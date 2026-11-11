---
name: verify-drags
description: "Model-check Terminal Delight's drags (the left bar's edge, INBOX rows, tabs, groups, panes, splits, the gauges' wheels, lights, tracks and sliders, FOCUS's zoom and selection, a card's scale, the Agents page) against a release the window never receives, with TLA+. Use before and after adding or changing a drag field, a drag's on_mouse_down, Workspace::on_mouse_move's no-button branch, or Workspace::on_mouse_up's chain in app/src/main.rs, or when a drag is reported stuck or coming back to life."
---

# Verify the drags

**The promise.** A press drags what it pressed on and nothing else comes along, and a release that reaches the window ends every drag.

**The harm if it breaks.** A drag the window never saw end (the compositor took the pointer mid-drag) comes back to life on the next press: a split or a slider moves that the person never grabbed.

## Run it

```bash
verify/check.sh verify/drags/Drags.tla
verify/check.sh verify/drags/Drags.tla verify/drags/Drags-before.cfg
```

The first is the code as it stands (`OneDrag = TRUE`, one `drag: Option<Drag>`): both rules hold and `findings.txt` is empty. The second keeps the code before 2026-11-11 (`OneDrag = FALSE`, a field per drag) and must still read `breaks (finding)` for both. The replay `a_lost_release_never_brings_a_drag_back_to_life` in `app/src/drag.rs` walks the bug path on the real window. Read a break as a path with the pstack `/statechart` skill.

## What it found (2026-11-10), and what fixed it (2026-11-11)

- **APressDragsOnlyWhatItPressed and AReleaseEndsTheDrag break, but only after a lost release.** Path: you press on a split (its drag starts) → the release never reaches the window → you press on a gauge slider without moving first. Now both fields are set; the next release takes the first in `on_mouse_up`'s chain and returns, and the other stays set until the pointer next moves with no button held. With every release delivered (drop `TheReleaseIsLost`) both rules hold, so these are findings about an event the window cannot rule out, not about every drag.
- **The fix:** `Drag` in `app/src/drag.rs` replaces the seventeen fields with one `drag: Option<Drag>` on `Workspace`. A press sets it, replacing anything a lost release left behind; the release takes it. Each old field is now an accessor of the same name (`drag_split()`, `take_drag_split()`, `set_drag_split(..)`), so every use reads as it did.

## Coverage

| Model action | Code |
|---|---|
| `YouPressOn` | each drag's `on_mouse_down` in `main.rs` (and `threshing_floor_view.rs` for `floor_drag`): sets its own field, reads no other |
| `YouRelease` | `Workspace::on_mouse_up`: `flyout.resize`, `bar_resize`, `inbox_drag`, `bar_drag`, the gauges' wheels, lights and tracks (one `||` chain that also stops at the first), `slider_drag`, `focus_zoom_drag`, `card_scale_drag`, `agents_drag`, `focus_drag_release`, `group_drag`, `tab_drag`, `drag_pane`, `drag_split`, each taken and returning |
| `YouMoveWithNoButton` | `Workspace::on_mouse_move`'s branches that see `pressed_button != Some(Left)` |
| `TheReleaseIsLost` | the compositor or another client taking the pointer mid-drag |

`Early` and `Late` stand for any two drags in the chain's order.

**Sync commit:** the commit that last changed this skill (`git log -1 --format=%h -- verify/drags/SKILL.md`). Check `git diff <that commit> -- app/src/main.rs` for `on_mouse_up`, `on_mouse_move` and any new `*_drag` field.

**Left out:** what each drag does while it moves, the right and middle buttons, the flyout's own resize (it is first in the chain and returns), and whether a real compositor loses the release (not measured).

## It can fail

The fix was broken on purpose: a press that adds to the drags instead of replacing them (`active \cup {k}` under `OneDrag`) makes `APressDragsOnlyWhatItPressed` read `BREAKS` on `Drags.cfg`.

## When you change the code on purpose

A new drag is a new `Drag` variant and a row in `drag.rs`'s accessors, never a new field on `Workspace`: a field outside `drag` is the bug this fixed.
