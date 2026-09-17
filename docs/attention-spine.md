# The attention spine

A narrow rail on the right edge that answers one question: **which of these
agents needs a person, and which one first.**

Parker runs about twenty-five agent panes at once. Before this existed, finding
the one that had stopped to ask something meant opening panes until you found
it — and the cost of that sweep is paid over and over, all day, mostly on panes
that wanted nothing. The rail exists to replace the sweep with a glance.

Six slices, all merged, `docs/plans/attention-spine/00-status.md` is the state of
record. This page is the map you need before changing any of it.

## What it refuses to be

These are not omissions. Each one was decided, and each is load-bearing:

- **It never acts on an agent.** It routes attention; it does not answer
  prompts, retry, merge, or send. Every specimen worth copying — Warp's review
  pane, HumanLayer's gates — replies to the agent from the rail. This terminal
  writes no bytes to any PTY, so ours cannot, and the constraint has held
  through every slice.
- **It never invents a number.** No zero stands in for a measurement nobody
  took. An unobserved age draws a dash, an unsourced review field prints
  `unavailable`, and a screen that could not be read says exactly that and
  nothing about what the agent is doing.
- **It never opens itself.** The closed spine is the steady state. The one
  exception is `TD_SPINE_DEMO=1`, which exists because the queue cannot
  otherwise be photographed — see *Traps* below.
- **It ranks each pane once.** The lane comes from `agent_badge`, the same
  precedence the tab strip wears. A second classifier here would give one pane
  two rankings, and the tab badge is the one an eye checks first.

## The shape of the thing

```
  a pane's screen
        │
        │  every 120ms, one grid walk (pane.rs)
        ▼
  ┌─────────────────────────────────────────────┐
  │ predicates          wants_human_row()       │  the row that matched IS
  │                     blocked_row()           │  the evidence — one walk
  │                     parse_status_line()     │  answers both questions
  └─────────────────────────────────────────────┘
        │            │
        │            └──► clip_evidence() ──► the quote, captured NOW
        ▼                                      (the screen may scroll later)
  needs_input / bell / bell_blocked / rail_state
        │
        ▼
  agent_badge(..) ──► rail_kind(badge, state) ──► Option<AttentionKind>
        │                                          the lane, decided ONCE
        ▼
  ┌─────────────────────────────────────────────┐
  │ Observation  { pane, kind, priority,        │  built per pane per frame
  │                origin, observed_at,         │  in Workspace::rail_build
  │                evidence, deliverable }      │
  └─────────────────────────────────────────────┘
        │
        │  attention::project()   sort: level → lane → age → pane
        ▼
  Vec<AttentionItem>
        │
        │  hold_order()   the order frozen while somebody is reading it
        ▼
  ┌──────────────┬──────────────────────────────┐
  │ closed spine │ the queue (overlay or dock)  │
  │ lane counts  │ rows, cursor, tray, links    │
  └──────────────┴──────────────────────────────┘
```

## Where each piece lives

| What | Where |
|---|---|
| The lanes, the sort, the lifecycle types | `app/src/attention.rs` — pure, no UI, no pane reads, no filesystem |
| Reading a screen; capturing the quote | `app/src/pane.rs` — `wants_human_row`, `blocked_row`, `clip_evidence` |
| The one precedence both surfaces read | `app/src/main.rs` — `agent_badge`, `rail_kind` |
| Building observations from live panes | `app/src/main.rs` — `rail_build`, memoised by `rail_rows` |
| Queue lifecycle (open, freeze, seen, close) | `app/src/main.rs` — `rail_toggle`, `rail_close`, `rail_key` |
| Painting | `app/src/main.rs` — `render_spine`, `rail_panel`, `render_rail`, `render_rail_docked` |
| Levels on the tree | `app/src/main.rs` — `level_of`, `set_branch_level`, `level_rows` |
| The declaring channel | `app/src/mcp.rs` — `declare_deliverable`, `validate_deliverable` |

`attention.rs` is deliberately the only file with no dependencies on the rest.
If a rule can live there, it should: that is where it gets tested without a
window.

## The invariants

Each one has a test that fails if you break it. They are listed with the test
name because a rule you cannot find the enforcement for is a rule that will be
broken.

1. **Unknown is not zero, and never counted.** An unreadable screen gets its own
   lane, is shown, and stays out of the number that says how many things want
   you. → `unknown_is_shown_but_never_counted`
2. **An unobserved age is not a zero age.** It sorts after every measured one
   and draws a dash. → `unknown_age_sorts_last_in_its_lane_whatever_order_it_arrives_in`
3. **A person's level outranks the machine's lane.** Promoted review sits above
   neutral decision. → `a_promoted_review_outranks_a_neutral_decision`
4. **Nearest explicit level wins, and unset is not neutral.** Unset inherits;
   neutral is somebody clearing one row inside a promoted branch. →
   `an_explicit_neutral_is_not_the_same_as_unset`
5. **The level travels down and never up.** A branch draws what it is set to,
   never what something inside it is set to. →
   `a_branchs_mark_is_its_own_setting_and_never_a_childs`
6. **The order holds still while somebody reads it**, and arrivals append rather
   than insert. → `a_row_that_arrives_while_the_queue_is_open_lands_at_the_bottom`
7. **A row's identity is its pane's, not its position.** →
   see `rail_build`; a positional key transferred a held order to a neighbour once.
8. **The quote is the line the predicate matched**, from the same walk. →
   `the_quote_on_a_row_is_the_line_that_set_the_flag`
9. **One list answers the cursor, the number keys and the click.** The unknown
   lane collapses, so "row 4 on screen" and "row 4 in the projection" differ on
   any real fleet. → `the_cursor_and_the_click_resolve_a_row_through_the_same_list`
10. **One door opens and shuts the queue**, because the lifecycle is three things
    and a call site that remembers two is a bug that looks like a feature. →
    `nothing_flips_the_queue_open_without_running_its_lifecycle`
11. **The overlay is not in the row that lays out the terminals.** Opening the
    queue cannot resize a pane, by construction. →
    `the_overlay_is_not_in_the_row_that_sizes_the_panes`
12. **The projection is built once per frame.** →
    `the_projection_is_computed_once_per_frame`

## How to change it

**Adding a lane.** `AttentionKind` is ordered by what it costs to leave a thing
alone, so where you insert the variant *is* the priority decision. Then:
`counted()`, `label()`, `reason()`, `source()`, `rail_ink()`, and `rail_kind`'s
mapping from the badge. The lane totality test will tell you if you missed one
of the first four.

**Adding a field to a row.** Ask first whether it is a function of something the
row already has. `reason` and `source` were fields for three slices before
anyone noticed they were `kind` wearing different words — two allocations a row
a frame, and two chances for a caption to disagree with its own colour. If it
*is* derived, put it on `AttentionKind` and let the row stay thin.

**Adding a review-tray field.** `Evidence::unavailable` is not a placeholder to
fill in later; it is the honest answer until a source exists. Adding a field
with no source is correct and useful — it makes the gap visible.

**Giving the rail a new observation source.** Go through `agent_badge`. Widening
`rail_kind` to read something the badge cannot see would re-introduce the second
ranking Slice 2 exists to have removed. If the badge cannot express it, fix the
badge — there is an open issue about exactly that, for live errors.

## Traps

Things that have already gone wrong here. They are cheap to re-introduce and
every one of them shipped green.

- **A positional pane key.** The projection keyed rows on a counter walked over
  tabs and panes, so every key shifted when any pane to its left opened or
  closed. Harmless until a held order and a seen-mark were keyed off it. Use
  `EntityId`.
- **A hit box computed as a fraction of its row.** The deliverable link's band
  was "the bottom 38%", which held until the review tray opened under the cursor
  and pushed the link four lines down. Measure the band; do not derive it.
- **A flag flipped without its lifecycle.** Four call sites set `rail_open`
  directly; one of them skipped the seen-mark, so a queue dismissed by clicking
  beside it left every row still marked new.
- **A carrier struct that derives `Default`.** `TabIdentity`'s doc promises a
  new field fails to compile rather than going missing — and it nearly did not,
  because answering the compiler with `None` satisfies it while breaking exactly
  what the doc describes.
- **A source-scanning test that counts its own text.** Two tests here count
  occurrences of a string in `main.rs`; both slice the file at `mod tests` first,
  because a check that matches itself passes at the wrong number forever.
- **`ctrl+shift+N` cannot be automated.** It is read by the focused *pane* —
  deliberately, since a focused terminal takes the chord first and a
  workspace-level binding would test green and do nothing. The cost is that a
  synthetic chord arrives with a shifted keysym the match on `"n"` declines, so
  the one gesture that opens this surface is the one a capture script cannot
  perform. `TD_SPINE_DEMO=1` opens the queue for that reason.

## What is still open

- The rail's own cost is unmeasured, though the projection is now memoised per
  frame rather than walked two to four times.
- `agent_badge` has no error arm while `hud::needs_you` counts one, so a live
  rate limit that has not yet rung is a needs-you to the parser and nothing to
  the badge.
- Most agents still report `Unknown`, because nothing positively identifies a
  resting one. The queue collapses them to a single line, which makes the flood
  survivable rather than solved.
- ~~A hosted pane's mode is announced once and never reconciled, so an agent in
  one is classified `SHELL` and is invisible to the rail entirely — not even in
  the unknown lane.~~ **Closed (#462).** The window re-reads the host's census
  every five seconds (`reconcile_modes`), so the `Mode` push is a shortcut
  rather than the only path — which mattered because, measured against a live
  host over forty seconds and 24 panes, that push fires *never*: an agent's
  classification is stable, so the one channel that could repair a wrong answer
  is the one that does not speak. A hosted pane nobody has described now holds
  `PaneMode::Unknown` rather than `Shell`, reaches the queue as
  `PaneKind::Unknown`, and draws a row reading **"Not described by the host"**
  in the unknown lane — shown, never counted.
- Held panes are out of scope, and deferred close is not built, so the exclusion
  has never been tested against real behaviour.
- The evaluation contract's baseline expired before it was recorded.
