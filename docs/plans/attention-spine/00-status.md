# Status: Attention Spine

**Difficulty: 4/10:** one optional surface over an existing pane wall; false confidence in the attention model could survive unnoticed, while the UI remains cheap to unwind → one combined gate.
**Turned out to be:** all six slices built and merged; the evaluation is the one piece that
cannot be done as written, because its baseline expired when the rail came off its flag.

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
- [x] **Slice 3, queue lifecycle — the queue holds still while you read it.** Fixed ordering
  (frozen on open, arrivals appended at the bottom, released on close), per-item evidence (the
  screen line the classifier matched, quoted at the scan that matched it), seen/unseen on the
  bell's own acknowledgement contract, and keyboard movement (up/down or j/k, 1-9, enter to the
  pane, `o` to open the deliverable, esc to fold).
  - **The order is held as KEYS, not rows.** A held row still ticks its age and still leaves the
    queue the moment it stops wanting anything; only the sequence is frozen. Arrivals append rather
    than insert where their urgency says, because a row that jumped in above the cursor is the
    defect the freeze exists to prevent.
  - **Evidence comes from the predicate, not from a second scan.** `wants_human` and `looks_blocked`
    are defined in terms of `wants_human_row` and `blocked_row`, so one walk produces the flag and
    the quote and they cannot disagree — they would only have disagreed on screens carrying two
    candidates, which are the ambiguous ones. A clean finish matches nothing, stores nothing, and
    the row omits the quote.
  - **Seen is keyed on pane AND lane**, so a reviewed pane that then blocks is unseen again, and the
    mark is taken when the queue CLOSES, from what the painter drew.
  - **Two defects found building it.** The pane key was a positional counter that shifted whenever
    any pane to its left opened or closed — harmless until a held order and a seen-mark were keyed
    off it, then a transfer of both to a neighbour; it is the pane's `EntityId` now. And the scrim
    click set the flag directly, skipping the seen-mark and the order release.
- [x] **Slice 4, review tray — and the deliverable finally has a channel.** `declare_deliverable`
  is a new MCP verb beside `leave_note`: an agent names the one artifact its turn produced and it
  becomes a plain click on that pane's row, with focus still a separate act. The tray opens on the
  cursor's row and shows deliverable / changes / checks / read-by.
  - **Three of the four fields say `unavailable` and that is the honest state.** Changed files and
    checks need an authoritative source that does not exist — a terminal sees a screen, not a
    working tree — so they are `Option` to the renderer and print the word at reduced strength.
    Shown missing rather than omitted, because an omitted row leaves a hole a reader fills in.
  - **Validation refuses what would ship working and be wrong.** A relative target is refused with
    its reason (it would resolve against the TERMINAL's directory, not the agent's); `javascript:`
    and `data:` are refused as not-documents. A label is optional and defaults to the target's last
    segment; a target is not optional.
  - The link measures its own hit band now. The old one was the bottom 38% of a row, which stopped
    holding the moment the tray opened under the cursor and pushed the link four lines down.
- [x] **Slice 5, attention levels in the tree.** Promote and Demote are the first two rows of the
  project, initiative and task context menus, in the same slot on all three. The level marks its row
  green-up or blue-down, travels downward with the nearest explicit setting winning, is persisted in
  the layout, and is the outermost key the queue sorts by — the ordering key that landed with Slice 1
  finally has a producer.
  - **Unset and neutral are different states** all the way to disk: unset inherits, neutral is a
    person clearing one task inside a promoted project. Pressing Promote on an already-promoted row
    clears it, which is how unset is reachable without a third menu row.
  - **The mark never rolls up.** Everything else on a bar row gathers upward; this one does not, or
    two rows would claim one instruction. Inherited is the same glyph at half strength.
  - `TabIdentity` caught one: it derives `Default`, so answering the compiler with `level: None`
    would have satisfied it while breaking the promise in its own doc — a task closing its last pane
    silently losing its level.
- [x] **Slice 6, responsive behaviour — the queue can dock.** `p` in the open queue docks it as a
  flex sibling between the panes and the spine; the preference persists and is separate from the
  fit, so tiling a window draws the overlay and widening it gives the dock back. Below 1280 logical
  pixels the pin is drawn inert rather than hidden.
  - **The geometry contract is structural now.** The overlay is not in the row that lays out the
    terminals at all, so "opening the queue does not resize a pane" is true by construction; the
    docked path is in that row, so it provably does cost them width.
  - The threshold was checked against this machine's real widths: 968 (a tiled pane) falls on the
    overlay side deliberately.
  - **The evaluation half is NOT done and cannot be.** It called for a recorded comparison against
    manual pane sweeping with the baseline taken before the rail became habit. The rail came off its
    flag on 15 September and has been on since, so that baseline no longer exists to take. Filed as
    `461 — The attention spine's evaluation baseline expired before anyone recorded it` rather than
    fudged, with the three invalidation criteria that would close it — one of which is Parker simply
    deciding the premise is settled by having used the thing.

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
