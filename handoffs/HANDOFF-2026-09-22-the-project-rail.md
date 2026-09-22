# Handoff — the project rail, its retro, and the landing chain (2026-09-22)

## Status

**Landed and installed.** Five pull requests merged in sequence overnight and this
morning — the rail (#677), naming + house frame + badge height (#681), every scrim
flattens the glass by construction (#682), the reviewer's concurrent scan (#684),
and the hindsight retro (#688). Main is `bb6a196`, built and installed as
`~/.local/lib/terminal-delight/td-bb6a196-main` with the launcher symlink
repointed. The wire files are identical to the running host (`td-7d41cb5-main`),
so **a window bounce loses nothing** — and a window keeps its old binary until it
is relaunched.

Nothing uncommitted. The worktree `~/Work/td-cutover`, its private target dir
`~/Work/td-target-rail` and the branch `rail/hindsight` were removed by the
consolidating agent after #688 merged. Session transcript package:
`handoffs/2026-09-22-the-project-rail.cdx`.

## What's done

| Change | Verified by |
|---|---|
| The top bar is a project-level git ticker — isolation badge, rotating frames, heartbeat — keyed by `EngKey` (project / group / unfiled), unchanged by tab switches | `the_header_corner_carries_the_mark_and_the_projects_state`; rig photographs in the brief |
| Declared vs observed drift (FOREIGN / VISITOR), afterglow of what just changed, the checkouts table on a click with the house frame | `apply_eng` afterglow tests; photographed at full width |
| The landing list — unpushed, touched files, `merge-tree` dry runs, idle worktrees, stashes, collisions — as "what must become true" | `the_landing_list_is_what_must_become_true` on a rig with a real conflict |
| `engineering_state` over MCP for a coordinating agent | `engineering_state_hands_an_agent_the_landing_list`; answered on the real rig through `ctl mcp from` |
| Every scrim over the glass flattens the warp by construction (`over_the_glass` + `warp::flatten()`), and a guard refuses one built any other way | `a_scrim_over_the_glass_flattens_it_by_construction` (mutation-proved), `flatten_is_sticky_for_the_frame_and_cleared_by_the_next` |
| The sweep reads every branch in turn; a calm project says nothing but its heartbeat; frames derived once per reading | `the_sweep_reads_the_active_branch_first_and_the_rest_in_turn`, `isolated_and_clean_is_a_tick_and_near_silence` |
| New tabs, projects and groups open their name box; Ctrl+Alt+R renames the highlighted row | `asking_for_a_tab_opens_its_name_box_and_a_by_product_does_not`, `ctrl_alt_r_names_the_highlighted_row_rather_than_splitting` |
| `scripts/land.sh <pr> <label>` — the landing chain in the repo | landed itself on its first run: 1589 tests on the merged tree, sha-checked install |

## How to run / verify

```bash
cd /home/parker/Work/terminal-delight/app && cargo fmt -- --check && cargo clippy --locked -- -D warnings && cargo test --locked
```

```bash
scripts/land.sh <pr> <label>
```

The visual rig is a **real window on its own session**, standing in the real
worktrees of this machine — a demo window has no left bar and cannot show the
rail. Layouts are hand-written in `~/.config/terminal-delight/sessions/rail{test,shut,group,warp}.toml`;
**only `railwarp` has `warp = 1.43`** — the others photograph flat, which is how
the bent table went unseen all night.

```bash
setsid env TD_SESSION=railwarp TD_RAIL_DEBUG=1 TD_RAIL_TABLE=1 /home/parker/.local/bin/terminal-delight &
```

```bash
env TD_SESSION=railwarp terminal-delight ctl mcp from railwarp - rpc '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"engineering_state","arguments":{}}}'
```

(the door prints a wrong-window refusal line first; read the last JSON line.)

## Not done / next

- `ctl rail` (#678) · pull requests via `gh` in the landing list (#679) ·
  hold-to-reveal topology (#680) · exempt only the overlay's rectangle from the
  warp instead of flattening the whole glass (#683) · the sweep keeps at most ten
  branches fresh (#689) · `land.sh` repoints without the ancestor check (#690).
- The frames are English-only; the corner truncates below roughly ninety
  columns; the help modal has no line for the rail.

## Watch out

- **Writers are panes.** An agent editing a checkout with no pane in it is
  invisible to the rail; a script or a person in another terminal never shows.
- **The launcher is contested.** During this tie-off a sibling had cut it over to
  a branch build (`td-cdb6492-qround`, PR #691); it now points at main at
  Parker's request. `scripts/land.sh` installs the *merged* main, never a branch.
- The four cadences — 2 s scan beat, 20 s stale, 6 s frame, 15 min afterglow —
  are guesses, not measurements.
- A fresh worktree is a cold gpui build (~23 min) unless you copy `app/target`
  to a private `CARGO_TARGET_DIR`; a *shared* one serves another worktree's test
  binary and silently drops your tests from the count.
- `is_calm()` is nine conditions chosen by hand; a state nobody thought of
  passes as calm.

## Where it's recorded

- Decision brief (the morning read): `~/Work/reports/2026-09-22-the-project-rail.html`
- Plan and gates: `docs/plans/project-rail/` (status carries the honest score)
- APES: `episodes/HANDOFF-2026-09-22-the-project-rail.md` (this file), the rail
  ticket closed with its deliverable, six follow-up tickets mirroring the issues,
  and the `apes remember` session context
- lean-ctx: session decision of 2026-09-22
- Memory: `a-hindsight-question-is-an-invitation-to-do-the-work`,
  `merged-is-not-installed` (now names `land.sh` and the reproducible build),
  `flat-over-the-glass-is-bought-in-one-place`,
  `a-real-window-on-its-own-session-is-the-left-bar-rig`,
  `merge-tree-answers-will-this-merge-without-a-checkout`,
  `mcp-from-is-the-shell-door-to-any-windows-tools`
