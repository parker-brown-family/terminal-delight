# ctl — scripting a RUNNING terminal

Every terminal-delight process listens on its own unix socket for one-line
commands, and the same binary is also the client. A live window is therefore
scriptable — from the desktop, a keybinding, a bar widget, or an agent's own shell.
Only the bench verbs and `adopt --run` reach a terminal's input (see Boundary).

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
| **MCP policy toggles + `mcp rpc`** | Hand a JSON-RPC line to the handler. All six global MCP policy changes are refused unless the window was launched with `TD_MCP_SCRIPTED=1` (decision 0003) | `ctl.rs` | `terminal-delight ctl mcp rpc '<json>'` |
| **`ping` / `paint status`** | Liveness and current overlay state, for bar widgets and scripts | `ctl.rs` | `terminal-delight ctl ping` |

## Boundary

`ctl` changes **appearance, tabs, notes and documents**, and **MCP policy only in a
window launched with `TD_MCP_SCRIPTED=1`**. Two kinds of command reach a terminal's
input, both on purpose:

- **The bench verbs.** `bench say`, `choose`, `submit` and `click` act on an agent
  pane's bench as a person would. `bench type` only fills the composer.
- **`adopt --run`.** It opens a new pane and types its command there.

None of them reaches a pane marked off-limits to agents. The full list of terminal-input
routes is [the security manifest](../security/terminal-input.md).

## Status

**Shipped.** The tab-strip architecture was handed over deliberately still open to
argument — see the handoff under `docs/handoffs/`.
