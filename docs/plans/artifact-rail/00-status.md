# Status: the Agent State and Artifact Rail

**Difficulty: 6/10.** A new surface over a store that already exists, one behaviour change in a verb every agent calls, and a door moved off the agent wall. The part that can be wrong without anyone noticing is the reply an agent gets when its open is held: an agent told `ok` when nothing opened, or told `err` and left to retry, misbehaves quietly. That buys **one combined plan page and one approval.**

- Plan (product, architecture, slices in one): **WAITING for approval.** The page is `reports/2026-10-22-artifact-rail.html` and carries five decisions, each with a recommendation. A concur with no note reads as approved as recommended.
- Builds on `dev/0.5.0` (the live window is `td-4e61d271`, dev/0.5.0 plus the palette PR), in the worktree `~/Work/td-artifact-rail` on `feat/artifact-rail`.

## Asked

Parker, 2026-10-22, from the pane titled "Bug - override glow" under TERMINAL DELIGHT: an agent finished its turn and opened its artifact in the floating square he was writing a note in. *"the behavior to overwrite the currently open glow pane when an agent completes its turn and escalates its artifact up to the user to read definitely will not override the currently open glow pane"*. ("Glow pane" is the floating square.) When a square is open, the top-right counter bumps instead. Clicking the counter opens the artifact rail instead of the agent wall: *"a global compilation of all of the agents with artifacts"*, as cards *"See how our Git coordination surface works"*, each card carrying *"the agent, its title, and the most recent artifact"*, newest first, persistent (*"not like the agent state attention spine … where clicking on it … takes that card off"*). Clicking an artifact *"will zip us over to that agent and open the artifact"*. The agent wall goes on the back burner: *"still keep the hotkey so that it could be looked at if someone is able to Easter egg and find that"*.

## Measured, 2026-10-22

- In the 24 hours to 09:42 PDT, 13 Claude sessions called `open_document` 37 times, every one with placement `here`. The servo tab (GLOBAL) made 8, including 08:43, 08:49 and 09:00 this morning. Counted from the transcripts' tool calls.
- How many of the 37 replaced a square somebody was using is **unknown**: `open_here` answers `ok float pane N` whether or not a square was open, and TD keeps no record of what a square showed before.
- `open_here` (`pane/doc.rs`) replaces any open square unless its notes are unsaved, and then answers `err …`. `declare_deliverable` never opens anything.
- 27 of the 34 agent panes in this window are titled `shell` (`list_panes`, 09:45). The left bar's tab names are distinct ("servo", "MCP work", "localized…").
- The counter's click sets `mcp_menu`, the agent wall's open flag. The wall's other doors are the `more-mcp` row in the `…` menu and Ctrl+Shift+A (`OpenAgentPanel`, pane.rs). The source test `the_agent_wall_opens_from_the_counter_that_summarises_it` pins the counter as its door.
- Each pane keeps its handovers in memory (`handover::Ledger`) and rebuilds them from its conversation's record. Nothing reads them across panes.

## Recommended decisions (on the page)

1. The hold applies to **every pane**, focused or not.
2. The counter bumps **on a hold**, and keeps a dot until the rail is opened. Not on every artifact.
3. A card is called by **its project and left-bar tab name**. A second agent in the same tab adds its sticky note's title.
4. A card **goes with its pane** when the pane closes.
5. The rail shows **this window's** panes.

## Slices

- [ ] Slice 1, the hold: in `open_here`, an agent's open over a square showing another file, or one holding unsaved notes, is held. The file goes into the pane's ledger (`hand_over`, `Declared` when it was not already there), the agent gets `held pane N — …`, and the workspace's bump counter advances. Removes the overwrite on its own.
- [ ] Slice 2, the rail: the counter opens it. One card per live agent pane with a non-empty ledger, newest handover first, order frozen while open. Card click: `pending_jump`. Artifact click: jump, then `open_float`, then the opened mark. Arrows, Enter, Esc. Empty state.
- [ ] Slice 3, the wall's doors: the `…` row and the help row go in all nine languages, Ctrl+Shift+A stays, and the counter-door source test is rewritten to pin the rail.

## Not in version one

Cards for closed panes, other windows' agents, `beside` splits an agent asks for, a new chord for the rail, removing or hiding a card, the agent wall's rework.

## Context

- `docs/plans/artifact-spine/` built the per-pane chip and listed "one list for the whole window: the same store, a second reader" as the next request. This is that reader, drawn as cards rather than a list.
- The card chrome is the coordination panel's (`coordination_view.rs`, radius 6, 3 px left edge), which is private to that file, so it gets pulled into a function both can call.
