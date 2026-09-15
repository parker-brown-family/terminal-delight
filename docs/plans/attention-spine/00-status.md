# Status: Attention Spine

**Difficulty: 4/10:** one optional surface over an existing pane wall; false confidence in the attention model could survive unnoticed, while the UI remains cheap to unwind → one combined gate.
**Turned out to be:** pending implementation and evaluation.

- Combined gate, Product + Architecture + Program Design + Slices: **APPROVED 2026-09-15** — via
  annotated brief (`reports/2026-09-15-attention-spine-groundwork.html` under `~/Work/reports`,
  six notes). "concur." Approval permits Slice 1. Three remarks were carried into `plan.md` the
  same day: held panes are out of version one, the five code amendments are written into the
  slices that own them, and a turn's **declared deliverable** becomes the review row's primary
  object with focus as a separate action.

## Slices

- [ ] Slice 1, tracer bullet: synthetic decision, failure, finished, and unknown observations reach the collapsed spine, queue, and existing pane-focus action.
- [ ] Slice 2, explicit live state: distinguish shell, agent, and unknown pane kinds; preserve unknown agent state through the current HUD parser; stamp the transition instant; derive the kind from `agent_badge`; give blocked a clearing edge.
- [ ] Slice 3, queue lifecycle: fixed ordering, per-item evidence, `project:group` from the tree, seen/unseen on the existing bell latch, and keyboard navigation.
- [ ] Slice 4, review tray: show only sourced changes, checks, and artifacts; render absent evidence as unavailable; carry the declared deliverable as a plain-click link, with focus as a second, explicit action.
- [ ] Slice 5, responsive behaviour and evaluation: overlay by default, opt-in pinning, geometry checks, and recorded comparison against manual pane sweeping.

## Prerequisite

Preserving unknown agent state is a precondition for Slice 2, not an argument against the rail:
the classifier currently ends every unmatched screen at `Idle`, which is also the enum's default,
so a queue built on it would report "nothing needs you" for a pane it could not read. Tracked as a
falsifiable issue on this repository.

## Notes for a fresh session

The annotated 2026-09-14 brief selected the human-handoff queue and review tray. Compact
`project:group` provenance belongs on their rows. The provenance tree is no longer a separate
destination. Risk radar is dropped. The 2026-09-15 blueprint added the deliverable contract and
put held panes outside version one.

This plan and its prerequisite issue were written first in a clone whose remote is a private,
month-stale repository; both were moved here on 2026-09-15. Read `docs/plans/` in this tree, and
file issues against this repository.
