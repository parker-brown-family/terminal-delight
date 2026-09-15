# Left bar manners — group-level and task-level behaviours

**Difficulty: 6/10**, scored before the reading and held. One destructive path
carrying a recovery promise, plus five changes of a handful of lines each. Six
buys one combined plan page and one approval, which is what this was: the page
is `reports/2026-09-14-left-bar-group-behaviours.html`, approved 2026-09-15 via
twelve annotations on it.

Pre-gate material: the left bar's own plan (`01-plan.md`), and the genesis issue
that left the project-row question open.

## What the annotations settled

| Question | Answer |
|---|---|
| Hold the processes now, or wait for the host? | **Hold now.** "If we can get A implemented SIMPLY, then let's go with it 100%." It is simpler than the alternative — see below. |
| Does deleting a project cascade two levels? | **Yes**, with the full count on the menu line. |
| Header `+`: commit, or tray? | **Both.** Left-click commits, right-click opens the tray. |
| Ship the five small ones ahead of the delete? | **Yes.** |
| Confirmation? | **On anything not empty**, worded like the close-tab dialog. An empty branch goes without a question. |
| Release rows | **Cut.** "do not release contents here! .. it is unclear and low utility." Same for the group row. |
| Colour on the tab row | **Cut.** "too much proliferation of colours and options." |
| The folder button | **Folded into the tray.** It drew in the inactive tone beside a live `+`, and both were about projects. |

## The A-versus-B judgement, since it was delegated

B — tombstone the layout, kill the shells, respawn on recover — *reads* simpler
because recording data sounds cheaper than keeping a process. It is not. B has
to capture every tab's name, working directory, pane tree and agent resume id,
serialise all of it, and then rebuild a split tree and re-run `claude --resume`,
inheriting every failure mode restore already has. And it does not do what was
asked: an agent mid-turn does not come back that way.

A is *don't drop the value*. `hangup` returns immediately without a host, so in
window-owned mode a close ends a shell by dropping the last reference to its
pane. Moving the `Tab` into a list instead of letting it fall out of scope keeps
it running, with no new mechanism at all. The store is a struct, a deadline and
a cap; the whole of it is `hold.rs`, and its policy is unit-tested to the corner
because it is generic over its payload and never looks inside it.

**The cost, stated plainly:** a holding dies with the window. The approved
close-undo spec promises otherwise, and that half needs the host to own the
pseudoterminals. The confirmation is therefore the first net and the four-hour
window the second, rather than the window being the only one.

## Retention

The numbers are the ones approved 2026-09-09 for close-undo, inherited rather
than re-decided: one pane held an hour, two or more held four, **counted in
panes rather than in the verb that closed them** — which is exactly why a
deleted group gets four hours without anyone choosing again — capped at the ten
most recent. The eleventh deletion evicts the oldest and hands its payload back
so the *caller* drops it. Nothing inside `hold.rs` ever ends a process.

## The three states that are not two

- A holding that expired and a holding that never existed are different answers
  (`Recovered::Expired` vs `Recovered::Unknown`). The tray never offers a dead
  one, and a recover that lands a moment late says which it was.
- The bay is shut / ajar / open, not shut / open. "There is somewhere to drop
  this" and "you are about to drop it here" are different things to say, and a
  door that only opens on hover is a door nobody finds.
- A tab row's "Remove from group" is drawn inert rather than hidden when the tab
  is in no group, so the menu does not change shape row to row.

## The bay

Reserved space at the foot of the bar, about two thirds the height of the
allowance slot. Two doors meeting at a centre seam, shut and blank — except for
a count when something is recoverable, because a trash whose contents are
invisible is a trash nobody opens, and somebody coming back for their work is
the entire point of the window.

A drag engaging parts the doors partway; the cursor entering opens them fully
and the bin lights. The doors ease toward a target rather than snapping to one,
so a drag that leaves mid-open closes from where it got to. The bay wins the
hit-test outright over any tree row beneath it: one gesture must not mean two
things, and the one it would silently pick is the one that does *not* delete.

## Deliberately not done

- A held branch does not survive the window closing. That needs the host, and
  lands with the client-server flip.
- No keyboard path to delete. The gesture is a drag or a menu; a chord that
  deletes a project is not something to add without being asked.
- Held panes are not paused. They keep reading, exactly as a background tab
  does — measured prior, not a measurement. If ten holdings ever cost something
  visible, that is where to look first.
