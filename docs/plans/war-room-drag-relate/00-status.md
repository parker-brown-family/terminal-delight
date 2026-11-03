# Threshing Floor drag-to-relate (drop-on-org popup) — status / ExecPlan

Difficulty: **6/10**. Builds on existing, merged drag infra (PR #1143) and the
cross-org data model (PR #1150). Net-new is a drop-target popup with three
branches and the MOVE mutation. Careful because the three choices each touch a
different mutation (membership vs org-line vs seat-line) and getting the drop
target resolution (org vs member) wrong silently relates the wrong pair — a
security surface. Design approved by Parker in the 2026-10-25 relationship-UX
brief (ask-2 concur) + 2026-10-26 "click drag a pane to build a relationship".

Parker's order (2026-10-26): **Build drag + land all 3, verify a green binary,
then STAGE it — he runs the live-window swap himself.** Do NOT swap the live
window.

APES ticket: `war-room-dropping-a-pane-on-a-separate-org-opens-a-move-relate-popup-mvpuau5y` (in_progress).
Branch: `feat/war-room-drag-relate` off `feat/war-room-system-map` (a9333fb2).
Worktree: `/home/parker/Work/td-authority-lab/war-room-drag-relate`.
Target dir: `/home/parker/.cache/td-targets/war-room-drag-relate` (reflink-seeded
from the warm war-room-system-map target; export CARGO_TARGET_DIR to build).

## Approved design (ask-2)
Dropping a pane card onto a **separate org** opens a popup at the drop offering:
1. **MOVE** the agent into that org (membership change).
2. **Relate to the ORG** (whole-org line — OrgLine End::Org).
3. **Relate to the MEMBER** dropped onto (seat↔seat — only when a specific
   member card was the target).
Dropping on a single pane card still builds a relationship as today.
The rung (See/Read/Ask + prompts-left) is chosen as the existing relation flow does.

## Landing plan (after build is green)
1. Merge #1149 (feat/war-room-relate-ux) → dev/0.5.0.
2. Merge #1150 (feat/war-room-system-map) → dev/0.5.0.
3. Open drag PR (feat/war-room-drag-relate → dev/0.5.0); after #1150 merges its
   diff is just the drag work. Merge it.
4. Build dev/0.5.0 → binary, verify green (fmt + clippy -D warnings + threshing_floor +
   threshing_floor_view tests + a render test).
5. Install the binary to a named slot and hand Parker the exact swap command +
   rollback. **He swaps.**

## State
- [x] Ticket opened + started.
- [x] Worktree + warm target.
- [x] Code map (Explore agent).
- [x] Build the drop-on-org popup + MOVE/relate routing.
- [x] Tests: 6 new view tests + 1 pre-existing contract test updated.
- [x] Gate green: fmt=0, clippy -D warnings=0, threshing_floor 52, threshing_floor_view 30.
- [ ] PR + land all 3.
- [ ] Staged binary + swap command for Parker.

## What was built
- `DropChoice`/`DropStage` state + `floor_drop` Window field (transient, unsaved).
- `threshing_floor_drop_on_org` replaces the org-button's direct `threshing_floor_assign`:
  same-org → select; loose pane → move in; seated-from-another-org → popup.
- Routes: Move (`threshing_floor_assign`, keeps its connection-loss confirm),
  Relate-to-org (`chart.draw_org(Seat→Org)`), Relate-to-member
  (`chart.draw(seat,seat)`, cross-org). Direction: dragged agent = from.
- 7 new `wr_drop_*` strings across all 9 locales; Esc + every sibling clear it.

## Verified facts
- Both #1149 and #1150 merge clean onto dev/0.5.0 and clean with each other.
- PR #1143 ("drag destinations visible + removal revokes access") is MERGED into
  dev/0.5.0 — drag already exists; this extends it.
- Live build is `td-047661b5-0.5-agaro-permissions` (from dev tip 3328924a); it
  has none of the round-2 strings.
