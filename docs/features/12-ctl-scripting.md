# ctl — scripting a RUNNING terminal

Every terminal-delight process listens on its own unix socket for one-line
commands, and the same binary is also the client. A live window is therefore
scriptable — from the desktop, a keybinding, a bar widget, or an agent's own shell
— without touching any PTY.

## Why it matters

A terminal you can only configure at launch goes stale the moment the day changes
shape. `ctl` is how the desktop and TD talk: the Omarchy bar widget that raises the
palette, the hotkey that labels a tab, the script that stands up a workspace and
names every pane in it.

## Features

| Feature | What it does | Evidence | Command |
|---|---|---|---|
| **Per-process socket** | `$XDG_RUNTIME_DIR/terminal-delight/ctl-<pid>.sock`, one per running window | `app/src/ctl.rs` module doc | — |
| **PAINT mode** | Raises the per-pane palette over every pane at once; the focused pane is spotlit, arrows walk the wall, a set's first letter paints it, `d` returns it to the desktop, `esc` folds | `ctl.rs`; `theme.rs::Dynamic::paint_chord` | `terminal-delight ctl paint toggle --workspace active` |
| **Scriptable tab strip** | Rename, label and organise tabs from outside the app, so a workspace stops being half-labelled | #304 | `terminal-delight ctl tab …` |
| **Self-addressing** | A tab edit aims at the window it is *running in*, not whichever is on screen — an agent scripting its own tab hits its own window | #305 | — |
| **Naming your own tab costs no arguments** | The zero-argument form targets the caller's own tab, and an op list survives being run twice | `ctl.rs` tab verbs | `terminal-delight ctl tab name <text>` |
| **Verb parsed before pane resolution** | A typo in the verb is reported as a typo, not diagnosed as an environment problem | `ctl.rs` verb-parse ordering | — |
| **MCP policy toggles + `mcp rpc`** | Flip the MCP read/write policy live, or hand a whole JSON-RPC line to the protocol handler | `ctl.rs` | `terminal-delight ctl mcp rpc '<json>'` |
| **`ping` / `paint status`** | Liveness and current overlay state, for bar widgets and scripts | `ctl.rs` | `terminal-delight ctl ping` |

## Boundary

`ctl` changes **appearance, tabs, notes and policy**. It does not write bytes to a
PTY — the same read-only boundary the [MCP surface](01-agentic-mcp.md) holds. The
one deliberate exception is opt-in and lives elsewhere:
[keepalive](11-usage-vitals-keepalive.md) types into an idle prompt.

## Status

**Shipped.** The tab-strip architecture was handed over deliberately still open to
argument — see the handoff under `docs/handoffs/`.
