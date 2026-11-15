# Status: Multibox (Terminal Delight on more than one machine)

Difficulty, submitted before any code: **9/10**. It crosses the session host's
trust boundary and the Threshing Floor's consent model, and a mistake in either
is silent: a far pid acted on locally, or one machine's line granting reach
into another. Post-hoc: **9**. The two bugs found were both of that silent
kind (a held pane reaped as ended; a save watcher alarming about a file that
lives on another machine).

Gates: **waived by Parker for this run** (2026-11-15: "build all this out end
to end without me - i am AFK so unblock yourself and take your lead from T3
Code"). This page is the combined plan and the record of the calls made.
Branch `feat/multibox`, cut from `dev/0.5.1` at 7be7bbda. Local only: not
pushed, no pipeline.

## What is built

1. **Open another machine's Terminal Delight here.** `terminal-delight remote
   <machine> [session]`, or Open here in Settings › Environments. The far
   session host keeps the terminals; ssh forwards its socket; a window here
   attaches to it. `terminal-delight machines` lists machines and sessions.
   Code: `app/src/far.rs`, and the far paths in `main.rs`.
2. **The Threshing Floor across machines.** A sending machine holds lines from
   its seats to far agents (`far_lines` in the session file); a receiving
   machine holds a ceiling per machine (`sharing.toml`). `list_panes`,
   `ask_agent`, `prompt_agent` and `prompt_status` cross machines. Code:
   `app/src/farfloor.rs` (rules, messages), `app/src/far_floor_view.rs`
   (window), `mcp.rs` (`far_door`, the `machine` argument).
3. **Settings › Environments** lists each machine, its sessions, the sharing
   chips, and this machine's own name.
4. **A pane shown in another window says `elsewhere`**, and Enter takes it back.

## Calls made without Parker (each is his to overturn)

| # | Call | Why | Undo |
|---|---|---|---|
| 1 | Terminals cross machines as state over an ssh-forwarded host socket, never as video. GLIFF stays what it is: the far desktop's pixels. | The 2026-10-12 intake brief's own finding, shelved "for now"; this run reopened it. | Delete `far.rs`; nothing else depends on the forward. |
| 2 | Opening a far session takes its panes from the far machine's own window for as long as the window here is open. | It is what attaching already means: most recent attach wins, per pane. | A read-only attach would need a host change (new protocol verb). |
| 3 | A far window never auto-reclaims a pane the far machine's window took back; Enter does. | Each side reads its own record of who holds the pane, so both would see permission and trade every pane every two seconds. | `reclaim_abandoned_streams`, one early return. |
| 4 | The receiving machine's ceiling defaults to nothing, and is per sending machine, not per agent. | The Floor's rule that nothing is reachable until a person says so. Per-agent limits already exist as pane positions. | `Sharing::ceiling`. |
| 5 | The ceiling is keyed by the name the sending machine gives itself. It is a claim, not a proof. | The road is ssh as the same person, who can already do anything on the far machine. T3 says the same of its links: routing, not isolation. | A pairing secret per machine pair would go in `sharing.toml`. |
| 6 | Far agents live in their own section of the Floor, with a chip per agent, not on the org chart canvas or the System Map. | The canvas and composer are bound to local panes throughout (12.7K lines); making a far agent a first-class card is its own project. | Additive: the section and `far_lines` would feed a canvas card later. |
| 7 | `ask_agent` to a far agent answers from that machine's last report (at most 30 s old, age stated), not a live round trip. No read-only copies across machines. | Nothing about a network is waited on in the window's thread. | `far_ask_out`. |
| 8 | A far window runs no local git scan, transcript binding, vitals or tool probe for its panes. Those rails read unknown. | Each reads this machine's disk; a folder of the same name here is a different folder. | Four guards in `main.rs`, each commented. |
| 9 | A far window does not start the save watcher. | It compares against a file that lives on the far machine. | One `if` in `build_attached`. |
| 10 | Not pushed, no pull request, no pipeline. | Asked for: "fully local". | `git push -u origin feat/multibox`. |

## Checked

- Unit and window tests: 14 `far::`, 12 `farfloor::`, 2 in `mcp::`, 1 held-pane
  regression (fails with its guard removed). Full suite at 6a56ce9e plus the
  clippy fixes: 3,704 passed, 0 failed. Clippy `-D warnings` and rustfmt clean
  at the head.
- Two-home rig (`~/Work/td-multibox/rig/`): attach, type, cut and mend the
  link (7/7 held and returned), handover of panes both ways, list / ask /
  prompt through the real MCP relay from inside a pane, and three negative
  controls on the ceiling.
- The real far box, over real ssh on the tailnet, against its released 0.5.2:
  attach to a scratch session, type a command that ran there, kill the ssh
  forward and watch the pane held and taken again. Its 0.5.2 window answered
  the census as an older build, and the Floor said so. The scratch session was
  removed.

## Not done, and not claimed

- **The cross-machine Floor has not run between two real machines.** The far
  box has no private display, and its real desktop was left alone. Both halves
  ran on one box in two homes, over the real control socket and MCP relay.
- A real agent's reply coming back across machines: the stand-in keeps no
  transcript, so the reply read as unknown, honestly. The reply path is the
  local one, unchanged.
- In a far window the launcher and composer still list this machine's
  projects. A launch there runs on the far machine, in a folder that may not
  exist there.
- The workbench, documents and artifacts of a far pane: they live on the far
  disk and read as empty here.
- Far lines on the System Map; a far agent as a card on an org chart.
- Tailscale as anything more than a reachable name: no Tailscale-specific
  discovery of machines.
- A machine with no Terminal Delight window open cannot answer the Floor; its
  session host alone can still be opened from here.
