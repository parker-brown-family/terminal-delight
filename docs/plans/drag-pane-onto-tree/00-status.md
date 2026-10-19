# Status: drag a pane onto the left bar's tree

**Difficulty: 6/10** — two subsystems (pane-drag, bar hit-testing) that
don't talk today, plus one new visual state (blocked/red) with no existing
precedent → one gate.
**Turned out to be:** 6/10 held. The two subsystems joined more cleanly than
expected (one field, one new match arm each); the real cost was exactly
where predicted — a brand new visual state with no precedent, and it hid a
real bug (`BarBranch::from(row)` collapsing every task row to `Unfiled`)
that static review caught before it ever ran.

- Gate — plan.md: APPROVED 2026-09-25 (via "Hundo - SEND IT!", read as
  covering both plans in flight; MAX_PANES capped on both paths per the
  0.61-confidence recommendation, not separately re-confirmed)
- Shipped: PR #816, merged 2026-09-25 (62613cf), installed as td-62613cf-left-bar-tree.
  Follow-up PR #819 (fix/pane-drop-cap-refuses-cleanly), merged 2026-09-25
  (5d7fa37), installed as td-5d7fa37-pane-drop-cap-fix — two rounds of
  review found two real bugs in the blocked-drop refusal logic (orphaning
  the pane on a truly-blocked target; wrongly refusing a pane dropped back
  onto its own already-at-cap tab), both fixed and unit-tested
  (pane_drop_capped). This plan's scope is now fully complete and shipped.

## Slices
- [x] Slice 1 — tracer bullet: `resolve_drop` returns the new `DropTarget` variants for bar rows
- [x] Slice 2 — blocked/allowed hover highlight (`pane_drop_hot`, wired into `branch_row` + `task_row`)
- [x] Slice 3 — `perform_drop` handling (the real moves; also deduplicated the tab-strip's own splice into `splice_pane_into_tab`, shared by both paths)

Verified: cargo check/fmt --check/clippy -D warnings all clean; full suite
1617 passed, 2 pre-existing unrelated `paneident::` failures. No new unit
tests added — `resolve_drop`/`perform_drop`/`bar_drop_target`/
`splice_pane_into_tab`/`pane_drop_hot` are all `&self`/`&mut self` glue
touching live GPUI entities, matching the house convention already set by
`move_tab`/`file_task` (untested directly; the pure logic they lean on —
here, none new beyond what slice 1 of the keyboard-carry plan already
added) is what gets unit tests.

**Honest gap:** the visual layer (row highlighting, the red blocked state)
compiled and passed every source-scanning meta-test in the suite, but was
never seen rendered — this is a GPUI desktop app and no live-window
verification ran this session. Worth trying by hand before calling the
visual polish done, independent of the data layer, which IS verified.

## Notes for a fresh session

Sibling plan in the same area, different input modality: keyboard-driven tab
carry is `docs/plans/left-bar-keyboard-carry/`. That plan's `file_task` /
`BarBranch` / `MAX_PANES` reading applies here unchanged — read
`apply_bar_drop` (main.rs ~11451-11511) first, it's the nearest precedent
for "take a drop target, mutate the tree, save."

Kanban ticket:
`drag-a-pane-onto-the-left-bar-s-tree-group-sibling-tab-tab-add-pane-w-max-panes--muh2rx8j`
(backlog — move to in_progress once plan.md is approved).

Key existing symbols this plan builds on, all in `app/src/main.rs` unless
noted: `resolve_drop`/`perform_drop`/`DropTarget` (16343-16481, currently
pane-grid-only), `drag_pane` field + its mouse-move/mouse-up handling
(~15909-15923, ~16164-16191), `bar_bounds: BarBoxes` (3967, already filled
every frame by the bar's own row-drag, just not consulted by `resolve_drop`
yet), `BarBranch` enum (~2908-2928), `file_task`/`file_initiative`
(10945-10979), `MAX_PANES`/`may_split` (top of file, ~130-160), `tab_pane_count`
(14857-14866).

Open question carried from plan.md's least-confident-decisions: does
`DropTarget::Tab{index}` (the existing tab-strip drop) start respecting
`MAX_PANES` too, for consistency with the new bar-row path? Needs an answer
before slice 3.
