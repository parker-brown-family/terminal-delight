# ChatGPT's prior-art survey, checked (2026-10-26)

Asked through Parker's ChatGPT in Chrome, at his request ("grab a chatgpt from the browser to offload and extend sources"). Conversation:

https://chatgpt.com/c/6adfd554-1794-83e8-9461-c012dd9214ff

## What to trust

- **The Deep research answer is not used.** Its header read "0 citations · 0 searches". Several of its claims were contradicted by its own follow-up or look invented (a "TabStripMaintainer" codename, a 25% reload figure for Chrome, "no public claims of memory leaks").
- **The web-searched follow-up is used**, through the pages it cited. Every cited URL answered HTTP 200 from this machine except three criu.org pages, which timed out together with criu.org's front page. The pages are listed in the brief's Sources modal.
- Its batch size for reclaim (32–64 MiB steps) and its predicate table are its own proposals, labelled as such by ChatGPT.

## What it added that changes the design

1. **Create a pane's memory group before the pane's first process starts.** Moving a parent later does not move children already running. Measured here too: D-Bus `StartTransientUnit` moves only the pids named. Adopting existing panes means naming the whole tree and checking afterwards.
2. **Keep the PTY master, the window and the supervisor outside the frozen group.** In Terminal Delight that is already true: the session host owns the PTY master and is not in any pane's group.
3. **Unknown pause-safety means no automatic freeze; unknown restart-safety means no automatic sleep.** Same rule as the brief's last refusal row.
4. **Single writer before resume.** Confirm the old agent process is gone before the conversation is reopened. This is exactly what probe2 broke on pane 111, and it is now a refusal row in figure 09.
5. **A frozen agent's own pressure readings say nothing.** Pace the push-out by the active agents' pressure, host I/O pressure and measured wake time instead.
6. **Codex's app-server documentation says an unsubscribed, inactive thread unloads after 30 minutes**, against 60 s in `config.schema.json`. The helper retest only reached 180 s, so the Codex leak claim is now "kept at least three minutes".
7. **`BUN_OPTIONS="--smol"`** is read by Bun standalone executables. Tested on Claude Code 2.1.296 (`probes/probe5.log`): two copies of the same 20 MB conversation side by side, 307 MB PSS without and 304 MB with after three minutes, 48 and 46 CPU ticks a minute idle. No useful effect.
8. Prior art it sourced: Android's freezer and compaction (process_madvise in the native code, CAP_SYS_NICE), Meta TMO and Senpai (pressure-driven offload through `memory.high`), Chrome tab discarding (rebuild on revisit, blockers for audio, forms, pinned tabs, connected devices), tmux-resurrect and Zellij (Zellij asks before running a resurrected command), iTerm2's long-lived job servers, CRIU (needs CAP_CHECKPOINT_RESTORE or CAP_SYS_ADMIN; no known-good setup for a Bun process with an MCP tree).

## Claims in the follow-up not opened here

- Claude `--resume` attaching to an already running background session.
- The CronCreate restore rules (they match the Claude docs read by the other research pass).
- Codex `turn/completed` firing for failed and interrupted turns.
- The `BUN_JSC_forceRAMSize` explanation, credited to a Bun issue whose title is about something else.
