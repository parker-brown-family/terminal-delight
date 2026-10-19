# Video clip overlay: status

**Difficulty: 5/10**, scored before any code. The change stays inside the document view and the pane's square, and if it is wrong it shows on screen at once. The hidden risk is a player or texture that never stops, and the video bench check measures that.

5 buys one combined plan page and one approval: `01-plan.md`.

| Gate | State |
|---|---|
| Combined plan | **approved with an amendment**, 2026-10-13. The video plays in its own box over the page and closes when it ends, not inline in the page |
| Slice 1: mpv once + finished, the two-box stack, Alt+click and `open_document` | done |
| Slice 2: page probe → a press on a video opens the clip | done |
| Slice 3: bench case + PR | done. Pull request 922; the Document face is left for issue 923 |

The amendment came from Parker's eye on the first draft, not from a rubber stamp. It changed the design from drawing the player inside the page's tiles to a second box above the page. The new design was smaller, because every control of the square kept working on whichever box is on top.

**Post-hoc score: 4/10.** Nothing was wrong for long. The one surprise on screen was a fault in the bench script, not the code: a retry pressed the clip it had just opened and paused it, and the first photograph showed it. The one surprise in the tests was a guard masked by a second guard, which the mutation run found and the test now pins.
