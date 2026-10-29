# Threshing Floor map parity: status

**Difficulty: 6/10.** One combined plan page and one approval. Four surfaces change (header, pool, org chart, System Map), and the map rework is real design. Wrong, it stays visibly wrong and costs a rework, but nothing in it touches authority: lines still grant exactly what they grant today.

## Where this came from

Parker, 2026-10-29, on the Floor after the boxed card vitals landed: *"The system map is a mess."* Three numbered notes, with seven lettered sub-items under the third:

1. An agent related to the org, or to any member, is shown outside the boundary.
2. The System Map follows the org chart's rules: a real map, clear boundaries between orgs, zoom *"very, very important"*, and the size of the work surface matters.
3. The header's top rows become a SINGLE row:
   - a) the counts go (the work-surface chip already has them);
   - b) buttons are grouped by function, different purposes are spaced apart, and hover gives a good description;
   - c) the search is small, at top right;
   - d) the hint text dies;
   - e) the bottom pad matches the top;
   - f) sleeping agents can be added (*"a sleepy agent should not be confused with a pure shell"*, and not woken on add);
   - g) the System Map gets a full rework for parity with the org chart.

## Gates

- Plan (`plan.html`): **APPROVED 2026-10-29 with amendments**, through notes on the figures ("yes", "yep - will have to be done carefully!", "nice"). No question was stamped, so all eight stand as recommended. Amendments:
  - **The THRESHING FLOOR title goes too.** *"maybe actually kill the threshing floor title. yes. not needed."*
  - **A child centres under its own parent, not under the row.** *"for a 2 - 3 - 4 tiered relationship WITHIN an org... the visual is CENTERED against the TREE, but should be centered against JUST ITS parent!"* His screenshot: omamaps, the child of X algo, sits under HISTORY's column. `layout()` places each row by sort order and has no notion of subtrees. This becomes the first part of slice 3.
  - **Connect goes.** *"CONNECT doesn't seem useful..."* A relationship starts from a card dropped on a card, a connector pulled, or a line clicked.
  - Wish, not scheduled: *"I really like the drag - putting on a vertical monitor would be super nice!"*. Waiting for Parker to say what he means.

## Slices

- [x] 1 · One-row header with no title: groups, hover text, hints out, pads match, footer becomes an overlay, Connect removed.
- [x] 2 · Sleeping agents count as agents in the pool and in relations; the card shows harness and model; no wake on add.
- [x] 3 · Tidy-tree layout (each child centred under its own parent); org border on the chart; an outside lane for related agents and orgs; whole-org lines drawn.
- [x] 4 · System Map on the shared canvas: N org borders, zoom 20–250% with three levels. **The relationship builder does NOT fold** (see below).

## Facts the plan stands on (dev/0.5.0 at fdacb175)

- Header: `render_threshing_floor` (header div); chips come from `mcp_switch_buttons` in main.rs; org controls from `threshing_floor_org_controls`. No tooltips on the Floor today. Every new `.tooltip(` call site must be added to the `known` table in `a_hover_card_over_the_glass_flattens_it_too`.
- Bottom pad: `threshing_floor_drag_footer` is always a fixed 60 px child.
- Pool gate: `mode.is_agent()` everywhere. An asleep pane is Shell mode with `rest.dormancy.asleep()` set; `prompt_party` already treats that as an agent.
- Org chart: wires need both ends on this chart, so a cross-org seat line is dropped; `org_lines` are never drawn there.
- System Map: `floor_system_map`, a flex list with no canvas, zoom or wires.

## Run log: Parker away, building to cutover (2026-10-29 afternoon)

Parker: *"build out EVERYTHING based on your own good judgement now... no human interaction... you will have completed the work and cut over!"*, with a screenshot showing the System Map still the old list.

- Slices 1–3 are LIVE on session 1 as `td-25d4c7ea-0.5-floor-slices-1-3`, swapped with the host kept. Released closed tabs: 117, 123 (ended) and 150, 152, 172, 180, 181, 184, idle shells with exited agents, per Parker's yes. PR 1194 (slices 1+2) is merged. PR 1195 (slice 3 + frame fix) is set to merge automatically once CI is green.
- Slice 4 is built on `feat/floor-system-map-canvas`, stacked on `feat/floor-tidy-tree-and-border`.

### Slice 4 design as built
- **The map draws on the org chart's viewport.** It shares `floor_view` zoom and pan, with the native scrolling removed. Zoom reaches 20%, up from 50%.
- **Each org is a bordered region** holding its seats, laid out by the same `threshing_floor::layout`. Orgs are packed largest first, left to right, wrapping at a world width.
- **Map cards have three detail levels by zoom:**
  - under 60%: a state dot and the name, the text kept legible;
  - 60–140%: place › tab, state, model line;
  - over 140%: adds the message line and the age and tokens.
- **Clicking a map card or an org's name opens that org's chart.**
- **Wires:** seat lines inside one org route like the chart. Seat lines across orgs and whole-org lines route as elbows between cards and borders, with a rung chip.
- **The map's builder form stays**, since it is the only way to draw cross-org and whole-org lines. The list of cross-org relationships, with Remove, moves into a drawer under the builder, closed by default.
- **The org view's relationship builder folds to a slim bar** until wiring starts: a card dropped on a card, a connector pulled, a line clicked, or the bar clicked.

### Slice 4 as shipped, and the one recommendation not built
- **The System Map is a canvas** on the chart's own pan and zoom. Each org is a bordered region with its member count, holding map cards in the chart's tidy tree. Orgs are packed largest first.
- **Lines on the map:** seat lines across orgs and whole-org lines route as elbows with a rung chip. Lines inside an org route as on the chart.
- **Cards and opening an org:** cards fall back to a state dot and a name under 60% zoom and add the message line above 140%. A card or an org's name opens that org's chart.
- **The builder and the drawer:** the map's builder sits where the relationship builder sits. Its list of cross-org lines, with Remove, is a drawer, closed by default.
- **`TD_THRESHING_FLOOR_DEMO=map`** opens the map for a rig.
- **Question 8, the folding builder, was tried and reverted.** Folding it into a slim bar either pushes the chart down when a draft opens, which breaks the approved "opening a draft never moves the page" (`composer_is_reserved_and_filter_stays_editable_while_choosing`), or floats the draft over the chart. Four builder tests showed the second choice covers the very cards a person must click next. The chart got its room from the one-row header and the drag footer instead. Options for Parker: a shorter composer (one row of slots); a draft that docks over the pool; or accepting a page shift.
- **Not photographed.** Both monitors were powered down (DPMS off) while Parker was away, so `grim -T` timed out for every rig, including the known-good slice 3 rig. Photograph after he is back: `docs/plans/floor-map-parity/evidence/floor-rig-map.sh <binary> <out> 1` with `TD_FLOOR_DEMO=map`.
