# Status: idle agents stop costing memory

**Difficulty: 8/10** — new authority over live agent processes (freeze, end, resume) in the host that owns every pane, where a wrong call silently loses someone's agent mid-work and the host cannot be restarted to undo it → four gates.
**Turned out to be:** 8/10 held. Gate 1 earned its keep (four amendments, none of them rubber stamps); Gates 2–4 were written as the record under "do all the work". What was misjudged was the kernel, not the product: memory charges stay where pages were first touched, which turned "push memory out" into "freeze, and be born in the group".

- Gate 1 — Product: APPROVED 2026-10-26 with four amendments (CONFIG › Agents page, sleep after 2 h of rest on a snapping slider, in 0.5.0, two new states in the agent counter) — `01-product.md`
- Authorization 2026-10-26: "do all the work" — Gates 2–4 are written as the record and NOT stopped at; build, test, install the rebuilt current build, coordinated with the MCP work
- Gate 2 — Architecture: written 2026-10-26 (`02-architecture.md`), not stopped at
- Gate 3 — Program Design: written 2026-10-26 (`03-program-design.md`), not stopped at
- Gate 4 — Slice plan: written 2026-10-26 (`04-slices.md`), not stopped at

## Slices
- [x] Slice 1 — every hosted pane in its own memory group; the header says what it costs (`panegroup.rs`, `memory_figure`)
- [x] Slice 2 — rest: gate in the socket, touch/output clocks, freeze + paced reclaim, RESTING chip, hud state, counter cell, orphan sweep
- [x] Slice 3 — sleep: wake file, SIGTERM, ASLEEP card, wake by key into a fresh pane, side-file store (not the layout leaf: 52 sites), restore keeps it asleep, dead-agent list
- [x] Slice 4 — CONFIG › Agents: 🤖 menu entry, rest/sleep switches and snapping sliders, launch defaults, MCP switches, agent colours, notices link, local-agent path; `agents.toml`
- [x] Slice 5 — gates (fmt, clippy both cores, 2,808 tests, 8 mutations all red), two off-screen rig runs, PR 1154 green in CI (Rust checks 13m40s) and merged to dev/0.5.0 as 604e7b1f, installed 2026-10-26 17:50 as `td-98ec53fe-0.5-idle-agents` (tree identical to 604e7b1f) with `live-window-swap-05`: host 2108452 kept, 32 host panes, 0 closed, split restored; within 20 s all 32 panes had their own `td-pane-1-*` scope

## Notes for a fresh session

- Asked by Parker 2026-10-26 in pane 109 (tab 18): idle agents that sit for up to a week cost ~500 MB each; make them cheaper inside Terminal Delight; "THIS IS A FUNDAMENTAL improvement and must be done RIGHT"; work with the Codex sibling in the same tab (pane 110).
- Measurements and probes from that session live in the brief's evidence modal and in `evidence/` beside this file. The probes ran on a disposable `--fork-session` copy in tmux with `TD_PANE_ID`, `TD_SESSION` and `TD_TAG` removed; no live pane was frozen, reclaimed or ended.
- Standing constraints found in the repo, which any design here must answer:
  - TD never types into a running terminal on its own initiative (#317, `docs/security/terminal-input.md`). Ending an agent must be a process action, never typed `/exit`.
  - `release-0.5.0` says 0.5.0 "does not add automatic agent creation, resume, forks, PTY input or process termination"; `agent-management/01-product.md` says that feature never resumes or replaces an agent process. Sleeping an agent is process termination plus resume, so its release target is Parker's call.
  - The host spawns every pane (`host.rs` `Host::spawn_pane` → `vt::pty::spawn`); agents are jobs of the pane's shell. No cgroup, memory accounting, freeze or idle state exists anywhere in the app today.
  - Issue #372 (host shares the window's systemd scope) is the same family of problem.
- Measured facts a fresh session can rely on (method in the brief's "Every number" modal, scripts in `evidence/probes/`): rest (cgroup freeze + `memory.reclaim`) took a copy from 525 MB to 11 MB in 2.5 s and woke to an echoed key in 191–212 ms; a plain freeze woke in 61–128 ms; sleep (SIGTERM) ended the agent and all its helpers in 0.48 s and a 20 MB conversation was back at its prompt in 1.4 s; an unprivileged process can move an existing pid into a new transient scope over D-Bus and freeze it. Codex's shared service kept a closed test window's helpers for at least three minutes; whether that is a leak or a 30-minute unload delay is open (`evidence/codex-daemon-keeps-every-windows-helpers.md`).
- Incident during the probes: probe2 resumed pane 111's live session for two minutes through a "newest file" fallback (two zero-cost `cost-state` lines in its transcript, nothing sent). Disclosed in the brief. Design rule taken from it: act only on processes inside the pane's own group, never on a conversation not bound with certainty.
- Codex sibling (pane 110, tab 18) has a sticky note pointing at the Codex evidence file; `ask_agent` on it resolved the WRONG conversation (`codex resume --last` is ambiguous across panes), so its summary cannot be trusted until it names its own thread.
