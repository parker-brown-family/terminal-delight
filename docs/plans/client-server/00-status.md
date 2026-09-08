# Status: Client-server split

- Gate 1 — Product: APPROVED 2026-09-08 — via annotated brief (13 notes,
  reports/2026-09-08-client-server-gate1.html); "approve with flags", all
  flags ingested into 01-product.md same day
  (out-of-scope section expanded 2026-09-08 on Parker's note: the two refusals now
  carry why-now, cost-to-reverse, and the constraint each puts on Gate 2)
- Gate 2 — Architecture: pending
- Gate 3 — Program Design: pending
- Gate 4 — Slice plan: pending

## Slices

(defined at Gate 4)

## Notes for a fresh session

- The feature: split terminal-delight into a long-lived server owning live
  sessions (PTYs, panes, state) and attachable clients (GUI, CLI, MCP).
- Gate 1 answers (Parker, 2026-09-08): problems = sessions survive the GUI +
  headless-first automation (phantom-window class dies); the ONE metric =
  lost sessions = 0 under a scripted kill-relaunch test; work lands in the
  live tree; v1 reach = this machine only; multi-attach explicitly not a v1
  goal (must not make v1 harder).
- Approval amendments (annotated brief, 2026-09-08): scrollback and
  non-resumable foreground programs (vim/htop) count in the metric; close
  semantics decided (pane/tab close kills — intent; app close preserves —
  server keeps everything); server-crash fallback is today's TOML recovery,
  recovery follows the agent session.
- Spun off, highest priority, NOT in this feature: the left-bar / workspace
  overhaul (vertical tabs+groups bar, tabs-as-tasks, project→epic hierarchy)
  — issue #319. v1 must not make it harder.
- Gate 2 emphasis (Parker): the grid read path seam is the one to be "extra
  careful" about — done right it unlocks presenting the terminal beyond a
  classic terminal UI downstream. Find and define that seam deliberately.
- These docs live in the LIVE tree (`~/Work/terminal-delight`). The checkout at
  `~/BROWN-FAMILY-SPORTS/Software/terminal-delight` is a stale fork (forked
  ~2026-08-18; see issue #313 "collapse the checkouts") — never build or deploy
  the installed binary from it. The 2026-09-08 phantom-window incident
  (#312–#314) came from exactly that mistake.
- Process: the 4-gate software-factory workflow,
  `~/.claude/skills/software-factory/SKILL.md`. Explicit approval at every
  gate; read every doc in this folder before continuing; resume from the first
  unapproved gate.
- Recon: DONE 2026-09-08 — `research-inventory.md` is the merged inventory
  (current process model, 13 existing IPC seams, 12 ranked coupling
  hot-spots, the state a server must own, reusable assets, 11 consolidated
  Gate 2 questions); `research-maps.md` holds the raw per-subsystem maps with
  file:line refs. Inventory only — no target design; Gate 2 designs against
  it. Its housekeeping flag (a live legacy `state.toml` writer) was checked
  same-day and is dead — residue only, cleanup rides the checkout collapse.
