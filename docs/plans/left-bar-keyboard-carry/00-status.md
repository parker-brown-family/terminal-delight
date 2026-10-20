# Status: keyboard carry — move a tab across groups and projects in the left bar

**Difficulty: 5/10** — one surface, a shape already built once (the
Ctrl+Alt+arrow tree-walk cursor), reusing existing mutation primitives →
one gate.
**Turned out to be:** 5/10 held — reused bar_walk/bar_enter/bar_leave's shape
almost exactly as scoped; the one real surprise was a source-scanning test
(`every_overlay_esc_can_close_also_flattens_the_glass`) that correctly caught
Escape-cancellation being wired through the wrong mechanism.

- Gate — plan.md: APPROVED 2026-09-25 ("Hundo - SEND IT!")
- Shipped: PR #816, merged 2026-09-25 (62613cf), installed as td-62613cf-left-bar-tree

## Slices
- [x] Slice 1 — tracer bullet: chord wiring + `bar_carry` state + zoom with no move
- [x] Slice 2 — real moves: Task/Initiative/Project-depth Up/Down
- [x] Slice 3 — edges: cancel, boundary clamps, carry-mode visual treatment (reuses `bar_cursor_ring`)

Verified: cargo check/fmt --check/clippy -D warnings all clean; full suite
1617 passed, 2 pre-existing unrelated `paneident::` failures (confirmed via
git-stash classification, unrelated to this change).

## Notes for a fresh session

Companion, unrelated fix already in flight alongside this plan (does not need
a gate — root cause confirmed by reading code, not a design decision):
`app/src/workbench.rs` / `app/src/main.rs`, clearing stale `Bench::selected`
on tab activation so the LAUNCH AGENT offer centers and Enter fires without a
click. Kanban ticket
`workbench-clear-stale-bench-selection-on-tab-activation-so-launch-agent-centers--muh2dmyb`.

Kanban ticket for this plan:
`left-bar-ctrl-shift-alt-arrows-carries-the-active-tab-across-group-project-sibli-muh2dl0n`
(still in backlog — move to in_progress once plan.md is approved).

Key existing symbols this plan builds on (all in `app/src/main.rs` unless
noted): `bar_cursor` field, `bar_walk`/`bar_enter`/`bar_leave`/`bar_jump`
(~11023-11254), `bar_rows` (10989-11021, the row-order source of truth),
`file_task`/`file_initiative` (10945-10979), `move_tab`/`nudge_active_tab`/
`move_group` (9335-9412), `apply_bar_drop` (11451-11511, the mouse-drag
equivalent — read this first, it's the nearest precedent for the whole
feature), `BarBranch`/`BarDragged` enums (~2908-3057), and `tree.rs`'s
`Row`/`RowId`/`Drop`/`walk`/`step`/`parent`/`first_child` primitives.
