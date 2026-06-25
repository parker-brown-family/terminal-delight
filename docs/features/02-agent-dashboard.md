# Agent dashboard / wall — the in-app HUD

The agent wall turns the read-only MCP data into a **living scoreboard**: one card
per agent, grouped by project, each showing model · effort · what it's doing right
now · live token spend · and whether it needs you. It's the in-app face of the
[MCP monitoring surface](01-agentic-mcp.md).

## Why it matters

A glance tells you the state of your whole fleet: who's working, who's blocked on a
decision, who errored on a rate-limit, who's done. Blocked/errored agents are
surfaced first — the wall pulls your attention to the agents that actually need it.

## Features

| Feature | What it does | Evidence | Status |
|---|---|---|---|
| **Status-line parser** | Extracts live metrics from each agent's own bottom status line (`✷ Accomplishing… (13m 18s · ↓ 59.0k tokens · high effort)`) — no API needed | `app/src/hud.rs::parse_status_line` (7 tests) | Shipped |
| **5-state classification** | Working ▶ · Blocked ‖ · Error ✕ · Done ✓ · Idle ○ — each a badge glyph + colour | `hud.rs` `AgentState::{badge,label,needs_you}` | Shipped |
| **Blocked detection** | Flags an agent waiting on a human decision (permission prompts, Y/N) vs merely thinking | `is_blocked_prompt()` 8-needle heuristic | Shipped |
| **Error detection** | Catches API errors / rate-limits (429, 529, overloaded, connection error) | `has_error()` 6-needle detector | Shipped |
| **Token parsing & format** | Parses `↓ 59.0k`, `1,234`, `1.2M`; formats compactly | `parse_tokens` / `fmt_tokens` (tested) | Shipped |
| **Gerund + effort** | Pulls the live action ("Refactoring…") and effort (low/med/high/max) | `extract_effort`, gerund parse | Shipped |
| **Grouped cards** | Cards grouped by project/tab-group; filter chips by **program** (Claude/Codex/Shell) and **state** (working/blocked/error/done/idle) | wall render in `main.rs`; chips | Shipped |
| **Needs-you highlighting** | Blocked/errored agents get a bright border + are drawn first | `AgentState::needs_you()` | Shipped |
| **Per-card theme + logo + art** | Each card wears its pane's theme colours and a logo/portrait; a **theme swatch badge** sits on the card's right frame edge | card paint; `register_overlay_tube` | Shipped |
| **Per-card warp** | The card's logo square bends with CRT curvature while the card stays a flat click target | `warp.rs` overlay tube (cap 32) | Shipped |
| **Rollup** | Fleet totals: ▶ working / ‖ blocked / ✕ error / ✓ done / ○ idle, Δ turn tokens, Σ session tokens | wall header | Shipped |
| **Click → watch** | Click a card to open a faithful read-only terminal facsimile of that agent's transcript | wall + modal | Shipped |

## The card anatomy (MTG/SWCCG-style portrait)

Frame in the pane's **theme colour** + a thin rim in the **program colour**
(Claude/Codex/Shell); inside: crest · name · **kind badge** (top-right) · cwd lore
strip · **art window** (logo, warped) · MODEL/EFFORT chips · the recent-message
feed · corner stats (Δ tokens · Σ session · status pip) · and the **theme swatch**
on the middle-right edge.

## Demo / honesty

All public demo wall data is **fictional** — no real prompts, paths, or business
ever ship in demo media. Demo logos/art are gated behind `TD_DEMO_LOGOS` /
`TD_WALL_DEMO` / `TD_DEMO`. See [ENV-FLAGS](ENV-FLAGS.md).

## Status

**Shipped.** The HUD is a read-only scoreboard — it never drives agents.
