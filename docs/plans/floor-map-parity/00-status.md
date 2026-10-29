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
- [ ] 4 · System Map on the shared canvas: N org borders, zoom 20–250% with three levels, relationship builder folds away.

## Facts the plan stands on (dev/0.5.0 at fdacb175)

- Header: `render_threshing_floor` (header div); chips come from `mcp_switch_buttons` in main.rs; org controls from `threshing_floor_org_controls`. No tooltips on the Floor today. Every new `.tooltip(` call site must be added to the `known` table in `a_hover_card_over_the_glass_flattens_it_too`.
- Bottom pad: `threshing_floor_drag_footer` is always a fixed 60 px child.
- Pool gate: `mode.is_agent()` everywhere. An asleep pane is Shell mode with `rest.dormancy.asleep()` set; `prompt_party` already treats that as an agent.
- Org chart: wires need both ends on this chart, so a cross-org seat line is dropped; `org_lines` are never drawn there.
- System Map: `floor_system_map`, a flex list with no canvas, zoom or wires.
