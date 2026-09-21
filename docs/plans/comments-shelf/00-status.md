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
| Combined plan (product + architecture + slices) | **approved**, with three amendments | 2026-09-18 |
| All four slices | **merged** in pull request 566 | 2026-09-19 |

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

**6/10, against a predicted 5.** The prediction was close and the gate it bought
was worth it — the plan page caught nothing, but Parker's ten annotations on it
changed three things before any code existed: no verb handing a comment to the
agent, a `copy` verb in its place, and typing-to-start-a-note on the board. Each
of those would have been a rewrite discovered in review.

What pushed it past 5 was not the feature. It was everything around it:

- **The plan's headline finding was wrong.** The three-tab strip was never
  clipping; a glyph-width estimate ran a third too fat. The invalidation check
  written beside the claim is what killed it, which is the process working, but
  a corrected claim costs a code comment, a test message, a pull-request section
  and a banner on the brief.
- **Two merges of a moving main**, the second of which spliced two independently
  added test functions into four interleaved conflict hunks. Resolving those hunk
  by hunk would have produced a test that compiles, passes and asserts something
  neither author wrote.
- **The verification rig could not reach the feature.** A pane id comes from the
  session host, so neither a scratch window nor a hostless one has a surfaces
  directory — the file-drop path is unphotographable without standing up a second
  host. Five tests replaced the photographs, and writing them found a third bug:
  `localtime_r` answering `3 Apr 584556019` rather than failing.

The lesson for scoring: what moved the number was not how much code there was,
but how much of the work was *checking*, and how much of the checking turned out
to be checking the checks.

## What is left, as issues

- **568** — a card copies the whole note; Parker asked to be able to highlight part of one
- **569** — a comment loses its signature across a restart, the first place issue 483 costs something
- **570** — a note cannot be saved on a hostless pane, because a pane id needs a session host
