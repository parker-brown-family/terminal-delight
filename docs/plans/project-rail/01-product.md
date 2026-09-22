# Product: the project rail

## Problem

In Parker's words, 2026-09-21: *"the top outer repeats something we already
see on the left spine."* The top row of the window named the branch the
active tab was in, directly above the tree row already lit for that branch,
and listed that branch's tabs, which the tree already lists as rows. One
fact, three times, in the most prominent places in the window.

What he wants there instead: *"a sort of GIT ticker for the PROJECT … the
outer rail should tell you about the engineering reality of the selected
project as a whole. Switching tabs does not change that rail. Changing
projects does."* And underneath it a thing no dashboard does: the tree says
where a pane is FILED; the disk says where it is WORKING; when the two
disagree, say so — *"filesystem-aware organizational linting."*

## Success metric

Two, both observable on this machine without instrumentation:

1. **A shared checkout is noticed before it bites.** Tonight, at 23:46, two
   agents were in `~/Work/terminal-delight` at once and a third's uncommitted
   hunks were swept into somebody else's commit earlier in the day. The rail
   says `1 SHARED` the moment a second writer lands in a checkout. Metric: a
   session in which Parker sees `SHARED` and moves an agent out — that is the
   number, counted by him.
2. **A misfiled pane is found by the rail, not by a screenshot.** The
   `DECOUPLE-ASYNC` pane is filed under Terminal Delight and sits in a
   separate clone of the repository under `BROWN-FAMILY-SPORTS/Software`;
   nothing in the window said so. Metric: the rail reports it on the first
   scan of a real session (it does — see the rig photograph in the brief).

## Announcement

Terminal Delight's top bar now belongs to the project you are standing in
rather than the tab you happen to have clicked. Beside the project's name
sits a small badge saying how many checkouts its panes are working in and
whether any two of them are writing into the same directory — `4 WT ✓` when
everything is isolated, `1 SHARED` when it is not. To the right, a ticker
rotates through what is actually going on: how much is uncommitted, which
branches are in flight and how far ahead of main they are, whether a pane
filed under this project has wandered into some other repository, and a
one-line summary in plain English derived from all of it. A tiny heartbeat
at the end shows the last hour's commits, so a hot project looks hot from
across the room. When everything is clean, isolated and quiet, the rail
says almost nothing — which is how you know.

## Screens

The mockups are photographs of the real thing on a real session, not
drawings — `mockups/` holds them, and the morning brief captions them.

- `01-tracer.png` — the corner names the project, the badge reads
  `5 WT · 1 SHARED · ⚠ 1 FOREIGN` off the real worktrees, one ticker frame
- `02-frames.png` — the rotation, one frame per photograph
- `03-tree-shut.png` — with the tree closed the tab strip is back, the
  badge stays
- `04-loose.png` — a session with no project: the mark and the badge alone
