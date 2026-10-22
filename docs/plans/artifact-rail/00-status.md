# Status: the Agent State and Artifact Rail

**Difficulty: 6/10.** A new surface over a store that already exists, and one behaviour change in a verb every agent calls. The part that can be wrong without anyone noticing is the reply an agent gets when its open is held: an agent told `ok` when nothing opened, or told `err` and left to retry, misbehaves quietly. That buys **one combined plan page and one approval.**

- Plan (product, architecture, slices in one): **APPROVED with one change, 2026-10-22.** All five decisions concurred, no notes (saved into `reports/2026-10-22-artifact-rail.html`). The change came with them, below.
- Builds on `dev/0.5.0`, in the worktree `~/Work/td-authority-lab/artifact-rail` on `feat/artifact-rail` (moved there so the build uses the lab's `zed-upstream`, which carries the image-glow patch the shared checkout lacks; `~/Work/td-artifact-rail` is a link to it). Target dir `~/Work/td-authority-lab/artifact-target`, reflinked from `clean-target`.

## The change Parker made at approval

*"Slight pivot - we will make a NEW glyph in the bottome right tray instead of repurposing the existing agent wall rail - SIMPLE SCroll (documents) ... consider this wil need to follow the design pattern of CLEAN."*

- The rail's door is a new **📜** glyph in the bottom-right corner, beside 🎨 📊 🪦 🔔 🐞 🧩. This work does not repurpose the agent counter, so the old Slice 3 (moving the wall's doors) is **dropped** from it (see Slices: the War Room took those doors).
- The bump and the dot (decision 2) move onto 📜, the rail's door.
- CLEAN's pattern, from `docs/plans/clean-ui-config/`: the glyph is a UI CONFIG switch (`[outer] artifacts` in `app/ui/all.toml` and `clean.toml`), one of the corner's menu glyphs (`Key::MENU_GLYPHS`, now seven), a chip in UI CONFIG's glyph row, and a row in CONFIG. Hidden, it gives the corner to CONFIG, and **its news moves with it**: the dot and the bump go onto CONFIG, the way the bug report's crash dot does.
- Clean hides it with the other six. Least confident of the calls here: the 📜 carries news, like the attention pill Clean keeps, but so does 🐞, which Clean folds.

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

- [x] Slice 1, the hold: in `open_here`, an agent's open over a square showing another file, or one holding unsaved notes, is held. The file goes into the pane's ledger (`hand_over`, `Declared` when it was not already there), the agent gets `held pane N — …`, and the workspace's bump counter advances. Removes the overwrite on its own.
- [x] Slice 2, the rail: 📜 opens it, following CLEAN (switch, chip, CONFIG row, news moves to CONFIG when hidden). One card per live pane with a non-empty ledger (a pane whose agent has exited keeps its card), newest handover first, order held while open with new arrivals on top. Card click: `pending_jump`. Artifact click: the pane's own `open_newest_handover` (the header chip's road), then `pending_jump`. Arrows, Enter, Esc. Empty state.
- ~~Slice 3, the wall's doors~~: dropped here. The War Room session (planning an org chart of agents for 0.5.0) told this one, the same afternoon, that Parker gave it the top-right counter as the War Room's door and the wall as an easter egg; it now owns the counter's click, the counter-door test, and the `more-mcp` row (repointed, not removed) and help row. Agreed by message; the only shared block is the `…`/CONFIG menu, where this branch adds `more-artifacts` after `more-plugins`.

## Not in version one

Cards for closed panes, other windows' agents, `beside` splits an agent asks for, a new chord for the rail, removing or hiding a card, the agent wall's rework.

## Context

- `docs/plans/artifact-spine/` built the per-pane chip and listed "one list for the whole window: the same store, a second reader" as the next request. This is that reader, drawn as cards rather than a list.
- The card chrome is the coordination panel's (`coordination_view.rs`, radius 6, 3 px left edge), which is private to that file, so it gets pulled into a function both can call.

## Built, 2026-10-22

Both slices on `feat/artifact-rail`, one pull request into `dev/0.5.0`. Proved in a hidden rig window with stand-in agents (`reports/2026-10-22-artifact-rail-build.html` has the pictures): five cards, newest first; a real `open_document` from inside a pane answered `held` with the person's square left up; the bump and the dot on 📜; Down and Enter going to the second card's pane and opening its artifact; under Clean, CONFIG listing Artifacts and carrying the dot.

Post-hoc difficulty: **6/10 held.** The one approval was not a rubber stamp: it moved the door to a new glyph and brought CLEAN's pattern with it, and it dropped a slice.
