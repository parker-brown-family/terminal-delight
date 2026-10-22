# Environment flags (`TD_*`)

Every environment flag Terminal Delight reads, grouped by purpose. Demo flags stage
**fictional** content only — no real prompts/paths ever ship in capture media.

## Core / runtime

| Flag | Effect | Default |
|---|---|---|
| `TD_THEME` | Override the theme file path | `~/.config/terminal-delight/theme.toml` |
| `TD_SOUNDS` | Fallback sounds dir for the bell clips | auto (next to binary) |
| `TD_ANCHOR_TOP` | Inverted read: prompt on top, recent→older down | off |
| `TD_SCRATCH` | Boot a scratch window (no master lock) | off |
| `TD_SEED_CWD` / `TD_SEED_RESUME` | Seed a single pane's cwd / resume command | none |
| `TD_SPINE` | **Turns the attention rail OFF.** The rail is on by default: a narrow spine at the right edge, `ctrl+shift+N` opens its queue. Set `0`, `false` or `off` to hide it; any other value (or none) leaves it on | on |

## Agentic / MCP

| Flag | Effect | Default |
|---|---|---|
| `TD_MCP` | Enable the read-only MCP server (agent-watch surface) | off |
| `TD_MCP_WRITE` | Turn MCP writes on at launch: appearance, notes, deliverables, surfaces and opened documents — never a terminal's input. Only with `TD_MCP` | off |
| `TD_MCP_SCRIPTED` | Let this window's control socket change the three socket-settable MCP switches (server, writes, expose) in either direction, for operator rigs that launch a window of their own. Without it `ctl mcp on/off`, `writes on/off` and `expose agents/all` answer `err`. Only `1` counts | off |
| `TD_GATEWAY_FD` | Set by `terminal-delight restricted run` inside the sandbox: the descriptor `restricted call` sends its typed requests on. Not for setting by hand | unset |

## Visual kill-switches / debug

| Flag | Effect |
|---|---|
| `TD_NOGLASS` | Disable barrel-warp rendering |
| `TD_NOCANVAS` | Disable CRT effects (scanlines/glow/etc.) |
| `TD_LATENCY=1` | Input→echo→parse latency probe (p50/p99 + throughput) |
| `TD_HITDEBUG=1` | Visual overlay for barrel-warp hit-testing |
| `TD_KEYDEBUG` / `TD_FXDEBUG` | Trace key dispatch / FX clock |
| `TD_USAGE_LIVE` | Open the </> usage face on the machine's REAL usage records (dev only — never for capture) |

## Demo / capture (fictional data only)

| Flag | Effect |
|---|---|
| `TD_DEMO` | Emit frozen lorem-ipsum demo content (spawned demo window) |
| `TD_DEMO_STATE` | Restore a demo `state.toml` instead of live state |
| `TD_DEMO_LOGOS` / `TD_WALL_DEMO` | Demo logos / fictional agents on the wall |
| `TD_WALL_THEME` | Override the wall theme |
| `TD_FOCUS_DEMO` | Auto-open the 👓 FOCUS modal on boot |
| `TD_DEMO_SEED` | Deterministic demo PRNG seed |
| `TD_GRAVEYARD_DEMO` | Fictional dead-agent list |
| `TD_SAVINGS_DEMO` | Demo mode for the LeanCTX savings plugin |
| `TD_USAGE_DEMO` | Fictional subscriptions on the </> card's usage face |
| `TD_CONFIRM_DEMO` | Auto-confirm certain dialogs for capture |

> Demo media rule: public captures must use staged, fictional content — never real
> prompts, paths, usernames, or business. (Enforced after a past leak; see the
> project memory.)
