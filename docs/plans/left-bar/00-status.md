# Left bar — status

**Difficulty: 6/10.** A UI reorganisation that adds a persisted layer and, for
the first time, lets the mother bar show fewer tabs than the session holds. The
tree itself is additive and reversible in one revert; the risk is entirely in
that second half, because a strip that hides a tab can hide an agent waiting on
a person. Six buys one combined plan page and one approval, not four gates —
and this run was built ahead of the approval at Parker's explicit instruction
("bias heavily on build now and I will iterate later", 2026-09-10).

| Gate | State |
|---|---|
| Plan (combined) | **written after the fact** — `01-plan.md`, this run |
| Implementation | **built**, on branch `left-bar` in `~/Work/td-left-bar` |
| Installed | see the handoff — versioned binary + symlink swap |

Pre-gate material: issue #319 (*Left bar: tabs and tab groups go vertical; a tab
becomes a task*), which holds the vision and the invalidation clause.

## What the score predicted, honestly

Six was about right, and for the reason the score is meant to catch: the
expensive question was not "can a tree be drawn" but "what happens to a tab that
is no longer on the strip". Everything hard in this feature is downstream of
that one decision, and it took an afternoon's worth of invariants (below) rather
than an afternoon's worth of layout code.

## The invariants, which are the actual deliverable

1. **The tree is complete.** Every task appears in it exactly once, whatever the
   scope, whatever is folded. Tested in `tree.rs`.
2. **You never lose your place.** The branches holding the active task refuse to
   fold, and activating a task from anywhere widens the scope to contain it.
3. **A hidden tab can still shout.** Branch rows roll up 🤖 / ✅ / ❌ / 📌 from
   everything beneath them, and the strip carries a `⋯n` chip counting what the
   scope is hiding — lit when one of them needs input.
4. **Unknown is not hidden.** A state file that predates the bar says nothing
   about it; that resolves to the default once, at load, rather than being read
   as "off".
5. **Filing is one fact in one place.** A grouped task's project is its group's;
   the tab's own field is written only when it has no group.

## Deferred, deliberately

- The main workspace still shows the active task's panes when a PROJECT row is
  clicked — clicking a project scopes the strip rather than opening a project
  overview. #319 left that question open and it stays open.
- No host-side knowledge of the tree. The whole thing rides in the session's
  layout file, which the host already stores and writes, so a second window on
  the same session sees the same tree with no protocol change.
- No keyboard navigation *inside* the tree (arrows to walk rows). The chord
  toggles it; the mouse drives it.
- The help modal has no row for ctrl+shift+B, because a help row added in
  English alone is worse than none (`lang.rs`). The `⟩` handle on the strip is
  the discoverable path back; the row wants a pass over all nine languages.
