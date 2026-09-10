# Left bar — the combined plan

One page, because a 6 buys one page. Written alongside the build, at Parker's
instruction to bias hard toward shipping; every decision below was made rather
than offered.

## The problem, in one line

The mother bar ran out of attentional space, and its answer — wrap onto a second
row — is the same problem stacked.

## The shape

```
┌ mother bar ────────────────────────────────────────────────────────┐
│ TERMINAL DELIGHT            [tabs in the CURRENT SCOPE]  ⋯3 🤖  + ▭ │
├──────────────┬─────────────────────────────────────────────────────┤
│ ∗       ⌁ ＋ ⟨│                                                     │
│ ▾ ● T-DELIGHT│                                                     │
│  ▾ ▮ client-s│                                                     │
│    │ host deb│              the terminals of the                   │
│    │ review  │              ACTIVE TASK, unchanged                 │
│  ▾ ▮ left bar│                                                     │
│    │ build 🤖│                                                     │
│    release no│                                                     │
│ ▸ ● BFS    ❌│                                                     │
│ ──── ⌁ ──────│                                                     │
│   scratch    │                                                     │
└──────────────┴─────────────────────────────────────────────────────┘
   the TREE (complete)          the SCREEN (one task at a time)
```

Three layers, and the third is the one that already existed:

| Layer | What it is | Where it came from |
|---|---|---|
| **Project** | a name, a colour, a fold | new — `Project` in `main.rs`, persisted as `[[projects]]` |
| **Initiative** | a run of tasks belonging to one push | the existing tab group, given a `project` field |
| **Task** | a tab: sub-terminals, agents, a sticky note | the existing tab, given a `project` field for when it has no group |

## Decisions

1. **A tab group IS an initiative.** Not a parallel concept. It already had a
   colour, a name, a fold and a contiguous run on the strip; the tree needed a
   middle layer with exactly those properties. One fact, one place: a grouped
   task's project is read from its group, and its own `project` field is written
   only when it is ungrouped.
2. **The tree is complete; the strip narrows.** This is the whole feature. If
   the bar were only a vertical copy of the strip it would cost width and buy
   nothing. Scoping is what stops the strip wrapping — and it is safe only
   because the tree is always whole beside it.
3. **Hiding a tab from the strip may never hide what it is saying.** Branch rows
   roll up the badge vocabulary the strip already uses, keeping the animation
   for the loudest state, and the strip carries `⋯n` for what the scope hides.
   herdr's own promise — *never hunt for the stuck one* — has to survive the
   narrowing, or the narrowing is a regression dressed as a feature.
4. **Absence is not a preference.** `left_bar` is `Option<bool>` on disk. A file
   written before the bar existed did not hide it; it never had the chance. That
   resolves to the default once, at load.
5. **No host changes.** The tree rides in the session's layout file, which the
   host already stores and writes for us. A second window on the same session
   reads the same tree with no protocol version bump — the layout schema is
   bumped only when the tree's SHAPE changes, and this only adds fields.
6. **Two layers, and no third.** Dropping an initiative onto an initiative is a
   no-op rather than a nested branch. #319's invalidation clause is about tasks
   fighting tmux, not about depth; depth is where trees go to die.

## What the guiding stars settled

- **herdr** (workspace → agent → pane) is the proof that the tree wants to carry
  agent STATE, not just names — its whole pitch is that you never hunt for the
  stuck one. Hence the roll-up, and hence the `⋯n` chip: a scoped strip without
  it would have quietly undone herdr's promise.
- **bb** ("Threads", "All Threads", "Pinned", a repo slug under each row) is the
  proof that a sidebar of work-in-flight wants a view mode and a home
  repository. `∗` (all) is the view mode; `⌁` adopting a task's git root as its
  project is the repo slug, made into a filing gesture rather than a label.

## Not built, on purpose

Clicking a project scopes the strip; it does not open a project overview in the
main space. #319 left "tabs for its initiatives? cards for its tasks?" open, and
answering it inside a build that was also inventing the tree would have been two
decisions in one commit.
