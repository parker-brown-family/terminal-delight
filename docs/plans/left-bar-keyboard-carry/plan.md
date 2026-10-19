# Plan: keyboard carry — move a tab across groups and projects in the left bar

**Difficulty 5/10** — one surface (the left bar's tree + its `on_key` chord
table), reusing a shape the codebase already built once (the Ctrl+Alt+arrow
tree-walk cursor is the read-only version of this exact gesture) and existing
mutation primitives (`file_task`, `move_tab`, `move_group`). No data-model or
persistence change. Cheap to unwind — one commit, no migration.

## What it is for

Moving a tab into a different group or project currently means the mouse:
drag it onto a tree row, or use a right-click menu. Parker wants a keyboard
path — grab the tab you're on, then walk it up through group → project (or
back down), landing it beside a sibling group or sibling project without
leaving the keyboard.

## The shape

Terminal Delight's left bar already has a **read-only** keyboard cursor for
this exact tree: Ctrl+Alt+↑/↓ walks rows, Ctrl+Alt+→ opens/steps in,
Ctrl+Alt+← closes/climbs out (`bar_walk` / `bar_enter` / `bar_leave`,
`main.rs:11023-11254`, cursor held in `self.bar_cursor: Option<tree::RowId>`).
This plan adds **Ctrl+Shift+Alt+arrows** as the "carry" sibling of that same
chord: same tree, same row order, but now the active tab travels with the
cursor instead of just the highlight.

**Re-scoped from the original description, and the one thing worth checking
before I build it:** the request as dictated was "press Ctrl-Shift-Alt, then
press plain arrows." Nothing in this codebase — or in gpui generally — binds
an action to a bare modifier chord with no other key, and the app's existing
tree-walk feature is the mirror image of that (Ctrl+Alt held, *then* an
arrow). I'm proposing Ctrl+Shift+Alt+arrow as one held chord instead, which
reaches the identical result (grab, then walk) through the mechanism that
already exists for the read-only version. If you want the bare-chord version
specifically, that's a different and riskier build (gpui may not even
deliver a keydown for modifiers alone) and is worth a separate look before
committing to it.

- New state: `bar_carry: Option<(usize, CarryDepth)>` beside the existing
  `bar_cursor` field — the tab index being carried, and which layer
  (`Task` / `Initiative` / `Project`) the arrows currently operate at.
- **Up/Down at Task-depth**: identical to today's `nudge_active_tab` (swap
  with the adjacent tab, clamped to the same group) — reused directly.
- **Left**: zoom out one layer (Task → Initiative → Project), mirroring
  `bar_leave`'s climb-out. Changes what Up/Down does next; moves nothing by
  itself.
- **Up/Down at Initiative-depth**: refile the tab into the previous/next
  sibling group *within its current project* — `file_task(i,
  BarBranch::Initiative(sibling_gid))`. Sibling order comes from `bar_rows`
  (the same order the tree draws and drag-reorder already uses — NOT raw
  `self.groups` order, which `bar_rows`'s own comment warns is different).
- **Up/Down at Project-depth**: refile the tab into the previous/next
  sibling project — `file_task(i, BarBranch::Project(sibling_pid))`.
- **Right**: zoom back in one layer, the reverse of Left. Not in the
  original description; added so there's a way back without cancelling.
- **Escape**, or activating a different tab, ends the carry.
- Sibling walking wraps at the ends (ring), matching `bar_walk`'s existing
  behaviour rather than `focus_dir`'s clamp-at-edge — same subsystem, same
  feel.
- Visual: `bar_cursor_ring` gets a second, distinct style for "carrying" so
  it reads differently from the plain highlight. No floating chip — that's
  mouse-drag's affordance for a different input device.

## Least confident decisions

1. ~~**Chord shape**~~ — **resolved.** Checked gpui's own source
   (`zed-upstream/crates/gpui`): a bare modifier press fires
   `ModifiersChangedEvent`, a type distinct from `KeyDownEvent`
   (`src/interactive.rs:62,656`), with its own listener registration
   (`src/window.rs:4417`) — and this app's `on_key`, its one keyboard entry
   point, never subscribes to it anywhere. The bare-modifier reading would
   need that entire event path wired up from nothing, plus invented
   semantics for partial-release (letting go of one modifier before the
   others). An adversarial Jev check against this same evidence (not asked
   to guess intent — asked whether the evidence supports the cost claim)
   came back 0.82. Proceeding with the held chord (Ctrl+Shift+Alt+arrow)
   unless told otherwise.
2. ~~**Scope: tabs only.**~~ — **settled, not just recommended.** Parker:
   *"groups will be locked into their project"* — a group cannot move
   between projects at all, by design, so there is no "carry a whole
   group" version of this feature to build later; a bar_cursor sitting on
   a Project/Initiative row stays a no-op for Ctrl+Shift+Alt+arrows,
   permanently, not just for this slice. (This surfaced a live
   inconsistency: `file_initiative` and the mouse group-drag path
   currently DO allow reassigning a group's project — flagged separately,
   not touched by this plan.)
3. **Which tab gets carried** — the live `bar_cursor` if it's resolvable to
   a task row, else the active tab. Mirrors the existing fallback in
   `rename_here` (`main.rs:11158-11178`).

## Slices

1. **Tracer bullet**: wire the chord in `on_key`, add `bar_carry` state,
   make Left/Right zoom (no move yet) with a visible depth indicator.
   Provable by pressing the chord and watching the ring/indicator change
   with nothing moving.
2. **The real moves**: Task-depth Up/Down (reuses `nudge_active_tab`);
   Initiative/Project-depth Up/Down (`file_task` + sibling lookup sourced
   from `bar_rows`).
3. **Edges**: Escape/cancel, clamping at zoom boundaries (no group to zoom
   into, only one project), and the carry-mode visual treatment finished.
