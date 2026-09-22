# Status: the project rail

**Difficulty: 8/10** — a new instrument reading git across many checkouts and
putting aggregate numbers in the most prominent row of the window, where the
failure mode is a confident wrong number that looks exactly like a real one.
→ four gates.
**Turned out to be:** _filled in at the end_

Opened 2026-09-22, overnight, on Parker's instruction: *"You are going to build
this out end to end as I sleep! Automated tests and everything … use a tracer
round to hello world the simplest possible build, then morph that hello world
into more and more complex pieces, then MVP, then full implementation. Manual
visual tests will be key."* The gates below are therefore **written as
artifacts rather than approvals** — the same footing as the seamless-restart
plan — and the morning read is the decision brief linked at the bottom, which
carries every least-confident call for him to argue with.

- Gate 1 — Product: written as artifact 2026-09-22 (`01-product.md`)
- Gate 2 — Architecture: written as artifact 2026-09-22 (`02-architecture.md`)
- Gate 3 — Program Design: written as artifact 2026-09-22 (`03-program-design.md`)
- Gate 4 — Slice plan: written as artifact 2026-09-22 (`04-slices.md`)

## Slices (see `04-slices.md` for what each one is)

- [x] 1 · Tracer — `0fbab3d`. Photographed: badge, one frame, heartbeat, the
      strip back with the tree shut.
- [x] 2 · Morph — `1a3d390`. Afterglow (pure diff, tested), warn wash, the
      checkouts table on a click, esc closes it.
- [x] 3 · MVP — the table photographed at full width; guards; committed.
- [x] 4 · Landing — the teeth: unpushed, touched files, `merge-tree` dry
      runs, idle worktrees, stashes, collisions, the landing list. Tested
      on a rig with a real conflict.
- [x] 5 · Exposure — `engineering_state` over MCP, answered on the real rig
      (see the brief). `ctl rail` NOT done.
- [ ] 6 · Pull requests via `gh` — not started.
- [ ] 7 · Hold-to-reveal topology — not started.

Not done and not attempted: localising the frames; the help modal does not
yet mention the rail.

## Notes for a fresh session

- Work happens in the worktree `~/Work/td-cutover` on branch
  `rail/the-project-has-a-nervous-system`, NOT in `~/Work/terminal-delight`,
  where other agents are live. Build with
  `CARGO_TARGET_DIR=/home/parker/Work/td-target-rail` — the shared target dir
  served another worktree's test binary (the trap on record), so the rail has
  a private one seeded by copy.
- The visual rig is a REAL window on its own session:
  `setsid env TD_SESSION=railtest <build> &`, whose layout is hand-written in
  `~/.config/terminal-delight/sessions/railtest.toml` and stands in the real
  worktrees of this machine. A demo window (`TD_DEMO`) has no left bar and so
  cannot show the rail.
- The screens are held awake by a `systemd-inhibit --what=idle` for eight
  hours from 23:50; check `hyprctl monitors -j` for `dpmsStatus` before a grab.
- Decision brief for the morning: `~/Work/reports/2026-09-22-the-project-rail.html`
