# Codex's shared service keeps a closed window's helper servers (at least three minutes; longer not yet tested)

Measured 2026-10-26 by the Claude in pane 109 (tab 18). Read-only except for one disposable Codex window it opened and closed in tmux, and the six helpers that window left behind, which it then ended by pid.

## What runs where

Codex 0.164 on this box runs a managed app-server daemon:

- supervisor `codex app-server daemon pid-update-loop` (pid 2094771, parent systemd --user)
- server `codex app-server --listen unix:// … --managed-daemon` (pid 3807336, started 2026-10-22 22:24, 110 children)

A Codex pane's own process (the TUI) has no children. It talks to the server, and **the server starts the MCP helpers**, one set per Codex window. Two TUIs from 2026-10-20 predate the daemon and still run their own seven helpers each.

## The leak, reproduced

`codexprobe.sh` (in the session scratchpad, quoted here):

1. Count the server's children: **110**.
2. Start a bare `codex` (no prompt) in tmux, with `TD_PANE_ID`, `TD_SESSION` and `TD_TAG` removed: the server starts **six** new children within 25 s — `bun` (garrison), `lean-ctx`, `cdx-mcp`, `workspace-mcp` (gmail), `node_repl`, `node` (cua).
3. `SIGTERM` the TUI. Thirty seconds later **all six are still alive** and the server still has 116 children.

Fresh, the six cost 261 MB resident: garrison 94 MB, gmail 114 MB, cua node 38 MB, lean-ctx 8 MB, node_repl 3 MB, cdx-mcp 0 MB. They were ended by pid afterwards; the server went back to 110.

**Retest against the documented unload delay.** Codex's config schema says a thread with no subscribers and no activity unloads after `thread_unload_delay_secs`, default 60. The first run only waited 30 s, so it was repeated at 15:55 with checks at 30, 60, 90, 120 and 180 s after the window's SIGTERM. One bare window made the server start **twelve** helpers (two full sets, all with cwd `/tmp`, the test's directory), and **all twelve were still alive at 180 s**. A third, partial set started in the same seconds with cwd `~/Work/cinema-delight`; it belongs to another agent and was left alone. The twelve were then ended by pid after re-checking cwd `/tmp` and parent 3807336 for each; the server went from 125 to 113 children. Whether the thread itself unloads while its helpers stay, or never unloads, is not established.

## What the kept sets cost now

The server's children by kind (count, oldest, newest): `lean-ctx` 19 (10-22 22:24 → 10-26 15:26), `node_repl` 19, `bun` 18, `python` 18, `cdx-mcp` 18, `node` 10, `codex-code-mode-host` 1, `terminal-delight` 1.

How many of those sets belong to windows that are gone is **not established**. An earlier draft said "at least ~11", counting only the 7 TUIs started after the server; that was wrong twice over: TUIs started before the server can reconnect to it after an auto-update (11 Codex TUIs could own sets), and one bare window was seen to start two sets.

Everything under Codex agents (server plus the two pre-daemon TUIs), PSS + SwapPss:

| helper | processes | resident | swapped |
|---|---|---|---|
| lean-ctx mcp | 21 | 50 MB | 3,202 MB |
| workspace-mcp (gmail) | 20 | 59 MB | 2,026 MB |
| garrison (bun) | 20 | 251 MB | 1,080 MB |
| cua node | 10 | 38 MB | 124 MB |

About 6.8 GB in all, nearly all swapped. Swap here is zram (zstd, 29.7 GB of data held in 11.3 GB of RAM, about 2.6:1, and full, spilling to `/swap/swapfile`), so swapped pages still cost RAM: roughly **2.9 GB of real memory**, more than every Claude Code process on the box combined (about 3.3 GB).

## Open questions — the Codex half

0. Which unload delay applies? `config.schema.json` says `thread_unload_delay_secs` defaults to 60 s; the app-server documentation (https://developers.openai.com/codex/app-server/, via ChatGPT, not opened here) says an unsubscribed, inactive thread unloads after 30 minutes. The retest only reached 180 s. Watch a closed window's helpers for 45 minutes before calling it a leak rather than a long delay.
1. Does Codex have a setting to end a thread's MCP helpers when its last client goes away, or after an idle period? (`codex app-server --help`, the config reference, the daemon's own docs.)
2. Is restarting the server safe for open TUIs (do they reconnect and resume their threads), and does a restart reap the sets?
3. Can these helpers be shared instead of per-thread: lean-ctx already runs a shared `lean-ctx serve` daemon (pid 2956880, itself 1.7 GB swapped); APES moved to one HTTP daemon for the same reason on 2026-09-22 (27 stdio copies, 1,178 MB).
4. Is this filed upstream with OpenAI? If not, a falsifiable issue with the reproduction above.

**Invalidation, run first:** if a TUI's exit is followed within five minutes by the server ending that window's helpers, the leak claim is wrong. Run step 2–3 above, note the new children's pids and cwd, and watch them for five minutes. Filter by cwd: other Codex windows start sets during the same seconds.
