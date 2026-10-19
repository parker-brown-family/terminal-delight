# Plan: drag a pane onto the left bar's tree

**Difficulty 6/10** — touches two subsystems that don't talk to each other
today (pane-drag's `resolve_drop` / `perform_drop` / `DropTarget`, and the
bar's own row-bounds hit-testing), and needs one genuinely new piece of UI
vocabulary — a "can't drop here" blocked indicator, which nothing in the app
currently draws. Reuses `BarBranch`, the same filing logic `file_task` uses,
`MAX_PANES`/`may_split`, and the "extract a pane into a fresh tab" fallback
`perform_drop` already has for an unmatched release. No data-model change;
cheap to unwind.

## What it is for

Dragging a pane (a running terminal — not a whole tab) currently only
understands the terminal grid and the tab strip: drop it on another pane to
split, drop it on a tab button to add it there. It doesn't understand the
left bar's tree at all — hovering a project, group, or tab row in the tree
does nothing. Parker wants three landings there:

- Hover a **group** row → the pane becomes a new tab, filed as a sibling
  under that group.
- Hover a **tab** row → the pane joins that tab as an additional pane —
  unless that tab already holds 4 panes, in which case a blocked (red)
  highlight shows instead and the drop is refused.
- Hover a **project** row → the pane becomes a new tab, filed directly under
  that project, ungrouped — the same result dropping onto Unfiled already
  produces today for a dragged tab.

## The shape

- `resolve_drop` (`main.rs:16343`) currently checks `tab_bounds`,
  `split_bounds`, `pane_bounds`, in that order. This plan adds a check
  against `self.bar_bounds` — the same `BarBoxes` registry the bar's own
  row-drag already fills every frame — cheap, since the bar and the terminal
  grid never overlap on screen.
- `DropTarget` gains two variants:
  - `BarTask { index: usize, blocked: bool }` — hovering a task row.
    `blocked = !may_split(tab_pane_count(index))`.
  - `BarBranch(BarBranch)` — hovering an Initiative, Project, or the
    Unfiled row. Reuses the existing enum rather than adding a new one.
- `perform_drop` gains matching arms:
  - `BarTask { blocked: false, .. }` does exactly what `DropTarget::Tab
    { index }` already does — splice the pane into that tab's root as an
    additional split.
  - `BarTask { blocked: true, .. }` is a no-op (kept so the match stays
    exhaustive; a blocked target shouldn't reach release in the first
    place if the hover state is drawn correctly).
  - `BarBranch(branch)` extracts the pane exactly the way the existing
    "unmatched target" fallback already does (`Tab::new(Node::Leaf(pane),
    None)`), then sets `.group` / `.project` with the same match
    `file_task` uses for `branch`.
- Visual: the blocked state needs a new hover style — a small red
  highlight on the row. Nothing else about the chip or tooltip needs to
  change.

## Least confident decisions

1. **Whether the existing tab-strip drop should also start respecting
   `MAX_PANES`.** Reading `perform_drop` today, dropping a pane onto an
   existing tab via the strip does *not* check `may_split` at all — it
   always splits. Capping both paths is the consistent reading of the
   constant's own doc comment ("eight is not a number of terminals anybody
   arranges on purpose — it is a number somebody arrives at by splitting
   once too often"), since a drag adds a pane by the identical repeated
   small action a keyboard split does. Checked this reading adversarially
   against the same comment with Jev — 0.61, a real lean but well short of
   the 0.82 the chord-shape reasoning got, so this is a genuine judgment
   call rather than a closed fact. Recommending the cap on both paths;
   flagging rather than assuming, since it changes existing behaviour.
2. **What "sibling with the group item" resolves to precisely** — a new
   tab filed under that group (`group = Some(gid), project = None`).
   Mechanically: `self.tabs.push(...)` alone would land it at the absolute
   end of the whole tabs vector, not necessarily contiguous with the
   group's other members — `move_group`'s own contract requires every
   member to stay contiguous, so the new tab needs to be inserted right
   after that group's last existing member (or via `move_group` after the
   push), not just pushed. No preference was given for *where* in the
   group's run (start vs. end); defaulting to the end since that is what
   falls out of "insert after the last member" with no extra logic.
3. **Blocked-state visuals** — proposing a red-tinted version of the
   existing row highlight, since nothing in the app currently draws a
   "can't drop here" state to copy from directly.

## Slices

1. Tracer bullet: `resolve_drop` recognizes bar rows and returns the new
   `DropTarget` variants — no visual change yet, no `perform_drop`
   handling. Provable by asserting the right variant comes back for each
   row kind.
2. Wire the blocked/allowed highlight for `BarTask`, and the plain
   highlight for `BarBranch` rows.
3. `perform_drop` handling for both new variants — the real moves, end to
   end.
