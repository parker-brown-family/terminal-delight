# Status: Attention Spine

**Difficulty: 4/10:** one optional surface over an existing pane wall; false confidence in the attention model could survive unnoticed, while the UI remains cheap to unwind → one combined gate.
**Turned out to be:** pending implementation and evaluation.

- Combined gate, Product + Architecture + Program Design + Slices: **APPROVED 2026-09-15** — via
  annotated brief (`reports/2026-09-15-attention-spine-groundwork.html` under `~/Work/reports`,
  six notes). "concur." Approval permits Slice 1. Three remarks were carried into `plan.md` the
  same day: held panes are out of version one, the five code amendments are written into the
  slices that own them, and a turn's **declared deliverable** becomes the review row's primary
  object with focus as a separate action.
  **AMENDED 2026-09-15 — attention levels.** Promoted / neutral / demoted, set by right-click on
  any left-bar row, inherited downward with the nearest explicit setting winning, marked in the
  badge line and never rolled up. Level is the outermost sort key, above the lane.
  **All four questions answered the same day** via the annotated levels brief: level outranks
  lane, a demoted row still counts, the nearest explicit setting wins, and the badge line takes
  glyphs and never numbers.

## Slices

- [x] **Slice 1, tracer bullet — merged 2026-09-15 (#424).** Synthetic decision, failure,
  finished and unknown observations reach the collapsed spine, the overlay queue and the existing
  pane-focus path, plus a declared deliverable opened by a plain click. Behind `TD_SPINE`.
  **Looked at on a screen 2026-09-15** — Parker ran a window with `TD_SPINE=1`, opened the queue,
  and the verdict was "I saw the rail and it was great". That closes the first outstanding item,
  and it is the only approval that could: the plan's whole claim is that a person stops sweeping
  panes, and nobody but the person sweeping them can say whether it does.
  **Closed 2026-09-15 — the flag came off.** The rail is on by default and `TD_SPINE=0` (or
  `false`/`off`) hides it, so the surface is the steady state and the variable only takes it
  away. `ctrl+shift+N` now has its help-screen row in all nine locales — nine, not the eleven
  this file claimed; `lang.rs` carries EN, ES, DE, ZH, FR, RU, JA, KO, HI and the number here
  was never checked against it.
  **Why an off switch at all**, when Parker's own reaction was "what maniac would ever turn it
  off": a switch that cannot be thrown is a claim nobody can test, and a demo, a screenshot or
  a plain disagreement all want the right edge back. It costs one line and one test.
- [x] **Slice 2, explicit live state — the rotation is gone.** The unknown half landed first
  (#425: `AgentState::Unknown`, the parser stops claiming idle for a screen it cannot read, both
  rollups draw it) along with the regression it exposed (#426: a finish bell promotes any quiet
  state, not only `Idle`). The rest lands here, carrying all three amendments:
  - **The lane derives from `agent_badge`**, so the rail and the tab strip read one precedence
    rather than two. The badge's own names are false friends and the mapping says so out loud:
    `AgentBadge::Blocked` is a finish that rang against a wall and takes the FAILURE lane;
    the live prompt is `NeedsInput` and takes DECISION. The whole badge precedence table is
    driven through the mapping in a test, so the two surfaces cannot drift apart silently.
  - **The transition instant has a producer.** The 120ms scan recomputes the lane once and stamps
    an instant only when it *changes*. A pane's first sighting stamps nothing — we know what its
    lane is, not when it became that — so `observed_at` stays `None` and the row draws a dash
    rather than claiming every pane changed the moment TD started.
  - **Blocked has a clearing edge.** A keystroke records the screen it was typed at, and the
    needs-you flag cannot re-arm while that exact screen is still showing. Keyed on the screen and
    not on a clock, because a stale prompt and a fresh one are the same thing to a timer. It is
    self-healing: an arrow key that moves a picker's selection changes the screen and re-arms on
    the next tick.

  **What Slice 2 deliberately took away.** Three fields the tracer synthesised now render as
  absent, because a slice whose claim is "explicit live state" cannot keep the props: the
  deliverable link is `None` until Slice 4 gives it a real source (a deliverable is something the
  agent *declared*, not a document this file picked out), every row is `Priority::Neutral` until
  Slice 5 puts levels on the tree, and `PaneKind::Unknown` is no longer constructed at all — this
  process launches its panes and knows what is in them. Removing the rotation is what exposed that
  those three had no other producer, and the compiler said so.

  **A divergence found on the way, not worked around.** `agent_badge` has no error arm, while
  `hud::needs_you` counts `AgentState::Error` — so a live rate limit that has not yet rung is a
  needs-you to the parser and nothing to the badge. Widening the rail's mapping would have hidden
  that behind the second ranking this slice exists to remove, so it is filed against the badge
  instead.
- [ ] Slice 3, queue lifecycle: fixed ordering, per-item evidence, `project:group` from the tree, seen/unseen on the existing bell latch, and keyboard navigation.
- [ ] Slice 4, review tray: show only sourced changes, checks, and artifacts; render absent evidence as unavailable; carry the declared deliverable as a plain-click link, with focus as a second, explicit action.
- [ ] Slice 5, attention levels in the tree: promote/demote rows on the project, initiative and task
  context menus; the level persisted in the layout; downward resolution with the nearest explicit
  setting winning; a green up or blue down mark in the badge line, drawn dimmer when inherited and
  never rolled up onto a collapsed parent. The projector's ordering key landed early with Slice 1.
- [ ] Slice 6, responsive behaviour and evaluation: overlay by default, opt-in pinning, geometry checks, and recorded comparison against manual pane sweeping.

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
