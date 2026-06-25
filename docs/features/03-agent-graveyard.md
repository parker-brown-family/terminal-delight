# Agent graveyard / recover — resurrect dead sessions

🪦 The graveyard scans your disk for agent transcripts that **no longer have a live
pane** — crashed, closed, or forgotten — and lets you **one-click resurrect** any
of them into a fresh pane that resumes exactly where it left off.

## Why it matters

Agent conversations are valuable and easy to lose — you close a terminal, reboot,
or a session crashes, and that context is "gone." It isn't: the transcript is still
on disk. The graveyard makes those recoverable in one click, turning a lost session
into a live, resumed agent.

## Features

| Feature | What it does | Evidence | Status |
|---|---|---|---|
| **Dead-agent scan** | Finds transcripts in `~/.claude/projects/*` and `~/.codex/sessions/**` with no live pane; newest-first, metadata-only first pass (cheap) | `app/src/recover.rs::scan_dead` | Shipped |
| **Head peek** | Reads the first 16 KB of each transcript to extract the **task summary**, cwd, age, and size — a minimal JSON parse, no regex | `read_head` + `json_str` | Shipped |
| **Resume synthesis** | Builds the exact resume command per agent: `claude --resume <uuid>` / `codex resume <uuid>` | `DeadAgent::resume_cmd` | Shipped |
| **Session-id safety filter** | Only surfaces shell-safe ids (UUID-like); nothing that could be a shell injection is ever typed | `safe_resume_id` whitelist | Shipped |
| **Age formatting** | Human "2h 17m ago" per entry | `fmt_age` | Shipped |
| **One-click RESURRECT** | Click a tombstone → spawns a pane, types the resume command, the conversation continues from the last turn | recover UI + restore path | Shipped |
| **Text-size scrubber** | Live font scaling of the graveyard list (same grade channel as the mother bar) | `card_scale` / grade `TextSize` | Shipped |

## How it ties together

The resurrect path reuses the **same restore machinery** as session restore
(`session.rs::agent_resume`) — so a graveyard resurrection and a normal crash-restore
behave identically. It never writes to or mutates the transcript; it only re-attaches.

## Demo

Fictional dead-agent lists are gated behind `TD_GRAVEYARD_DEMO` for capture media.

## Status

**Shipped.** Localized across all 9 languages (issue #75 routed the dashboard +
recover strings through `lang.rs`).
