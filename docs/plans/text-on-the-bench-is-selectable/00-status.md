# Text on the bench is selectable — status

**Difficulty score: 6/10.** The bench already records a flat rectangle per control and
un-bends the pointer to look it up, so the transport for this exists. What does not is a
gesture that competes with it: a drag has to start somewhere, and the two places worth
dragging over — the card body and the rail rows — are both already zones that act on
mouse-down. A selection that drops one run out of twelve reads as working, and would for
weeks. A 6 buys one combined plan page and one approval, not four gates.

**The combined plan page is the brief**, not a doc in this folder:

```
reports/2026-09-21-text-on-the-bench-is-selectable.html
```

It carries the fence, the anatomy of a drag, the architecture built up in four frames, the
press decision tree, the frame-ordering argument, the clipboard join table and the slices —
drawn. Parker annotates it in the browser and the note map comes back as
`read my notes in <file>`.

## Gate state

| Gate | State | Date |
|---|---|---|
| Combined plan (product + architecture + slices) | **awaiting approval** | 2026-09-21 |
| Re-scoped against the decouple and re-read on `origin/main` | revised, still awaiting | 2026-09-21 |

## Two corrections, both found after the first draft

**The first draft was written against a tree 66 commits behind `origin/main`.** My own branch
had been merged as pull request 596 and main had moved on; across the four files this plan
touches, those commits are 3,645 insertions and 971 deletions. Everything has been re-read with
`git show origin/main:`. Every structure the plan rests on survives unchanged in kind — `zone`,
`probe`, `micro`, `Slots`, `Zone`, `hit_at`, `dial_drop`, `Hit::Arm`, `Hit::OpenRow`, `unwarp` —
and nobody built a selection in the meantime. The call-site counts grew: **80 `micro()` sites
and 75 text-bearing leaves on main**, against 73 and 71 here.

**The composer is out by default.** The pane carrying the workbench/terminal decouple
(`docs/plans/workbench-drives-the-agent/`, scored 9/10, four gates, Gate 1 awaiting approval)
recorded Parker's ruling that no more repairs go into the mirror — the fifteen small ones that
brief costed out were cancelled because they would be thrown away. The composer *is* the
mirror, and this plan's one composer decision (read-only, because it cannot type over a range
the far end does not know about) is a constraint the decouple removes. He asked for the
composer by name, so it is a question on the brief rather than a silent cut.

What survives the decouple, and why: the atom list, the pure selection model, the drag, the
overlay and the `Hit::OpenRow` deferral are all on the presentation side of the seam, which is
the side that does not move. `Hit::Arm` is entangled — it means "start talking into the mirror"
— and gets re-specified by the decouple's Gate 2 rather than here.

## The ask, in his words

> *"If you are able to mouse text, click and drag to highlight any of the main work area items
> or right bar items from the workbench, I don't care about the menus … if the right side shows
> a comment summary that I wish to click, drag, highlight, and then copy and paste, I would
> like to be able to do that … including the text entry user text box, but also the main work
> surface and the right spine surface for the cards that are shown there."*

Two regions: the card body and the rail. The strip, the shelf tabs, the gallery and the pane
header are out by his line. The composer was a third until the decouple ruling landed — see
above.

## Slices, in order

1. **Every text run says where it is.** `benchdraw::sel` returns a `StyledText` and registers
   its `TextLayout` plus its string into `wb_atoms` at BUILD time — build order is reading
   order. `micro()` routes through it, which covers 80 of the call sites in one change; the
   remaining direct `.child(<string>)` leaves in the body and rail are converted by hand.
   Two `probe` calls measure the body and rail rectangles. No gesture yet: the
   slice ships `TD_SELDEBUG`, which prints the run under the pointer, because a run missing
   from this list is missing from every copy afterwards and nothing later would catch it.
2. **The model, pure and gpui-free.** `Atom`, `Caret`, `Sel` in `workbench.rs` beside `Zone`
   and `hit_at`: document order, the partial ends, the nearest-run fallback when the pointer
   is in the gutter, the join rule that decides a space from a newline, and the validity check
   that drops a selection whose anchor no longer holds the text it held. Table tests, no
   render.
3. **The gesture.** `on_mouse_down` arms an anchor for `Hit::Arm` and `Hit::OpenRow` instead of
   acting; `on_mouse_move` extends; `on_mouse_up` either fires the deferred click (moved under
   ~5px) or settles the selection. The highlight is an overlay of absolutely-positioned rects
   in the bench root's own coordinates, built from the PREVIOUS frame's atom list — read at the
   top of the build, immediately before the list is cleared, which is what `dial_drop` already
   does with the dials.
4. **Getting it off the bench.** PRIMARY on release, `ctrl+shift+c` in `bench_key`, and the
   right-click Copy row. `ctrl+c` is untouched — `window_chord` already records why: it is the
   only way to interrupt a running agent.

## Questions the brief asks

1. Presses on the card body and rail rows act on release. Accept?
2. The composer: still wanted, knowing the mirror it is made of is being replaced?
3. Verb chips inside a card: highlightable in passing, or holes in the selection?
4. Anything else on the bench that should be inside the fence?

## What the score predicts

Two gates' worth of ceremony was collapsed into one page on purpose. If this approval comes
back as a bare `go` with nothing questioned, the score was a point high and the next surface
change of this shape gets a plan paragraph rather than a page. Recorded here so the prediction
can be checked rather than remembered.
