# Comments shelf — status

**Difficulty score: 5/10.** A new tenant in a building that is already framed. The
shelf abstraction, the file transport and the row renderer all exist; the only
genuinely new mechanism is a text box that must NOT write to the pseudoterminal.
A 5 buys one combined plan page and one approval, not four gates.

**The combined plan page is the brief**, not a doc in this folder:

```
reports/2026-09-18-comments-shelf-and-spine-dry.html
```

It carries the product shape, the architecture, the DRY audit, the four
questions and the slices, drawn. Parker annotates it in the browser and the
note map comes back as `read my notes in <file>`.

## Gate state

| Gate | State | Date |
|---|---|---|
| Combined plan (product + architecture + slices) | **awaiting approval** | 2026-09-18 |

## Slices, in order

1. **The tab row holds what it is given.** `flex_wrap` on the tabs div in
   `pane/bench.rs`, plus a test over `rail_fit`'s widths asserting every shelf
   in `Shelf::ALL` is reachable. Breaks first on today's three tabs.
2. **A comment exists and survives a restart.** `Kind::Comment`,
   `Shelf::Comments`, `Tint::Mine` → `Role::Human`, `Origin::Person`. Proven by
   a hand-written `.json` before any UI — the tracer bullet.
3. **You can write one.** A note composer that is a real buffer rather than a
   mirror of the agent's line editor, on `alt+m` and on a `+ note` affordance.
   The test that matters: composing a comment sends zero bytes to the pty.
4. **The corner guard, and the two corners.** The source-scanning test
   `benchdraw.rs:25` already claims exists, mutation-tested before it is
   trusted; then the two `rounded(px(3.))` literals go through `sk.rad_raw`.

## Open questions, answered on the brief

- Does a comment ever reach the agent on its own? (recommended: no, with an
  explicit `ask agent` verb)
- Per pane, or per project? (recommended: per pane, as surfaces already are)
- Does `comment` go in the agent-facing catalogue? (recommended: no)
- Is the tab-row fix inside this work or its own issue? (recommended: inside,
  as slice one)

## Honest post-hoc score

_To be filled when the slices land._
