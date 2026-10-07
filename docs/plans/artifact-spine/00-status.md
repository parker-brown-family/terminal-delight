# Status: the artifact chip

**Difficulty: 5/10.** A chip in every pane's header over machinery that already exists: the bench store, the floating square, the screen scans that keep what you asked. The expensive call is filing every handover into the agent's bench, because once agents' shelves carry them, changing where they live is a migration. That buys **one combined plan page and one approval.**
**Turned out to be: 5, and right.** The approval was a single sentence and the build ran in one night with no decision coming back. What earned the five was the screen rule, not the chip: two of its hard cases were found only by photographing a real window, after every test was green.

- Plan (product, architecture, slices in one): **APPROVED 2026-10-06.** Parker: *"Cool - BUILD IT!"*, with one direction on the overlay: *"I like [the file open in the floating square] as a model -- even if the artifact hisotry USES a md file??? but anyways, opening artifacts from there is to be a single naked left click (not alt+_click) for opening INTO the pane"*. No notes on the brief, so all four decisions stand as recommended. `01-plan.md` is the plan as built; the brief keeps the proposal.
- **Redrawn per pane, 2026-10-06.** The first draft was one list for the whole window, a second pill under the needs-me count. Parker: *"What if the chip is PER PANE (not tab not project etc...) rather than global?"* The first draft's case for one list was that he went looking without knowing which pane held the link; he had in fact scrolled up inside one pane, so that premise was wrong. The folder keeps its `artifact-spine` slug so the links already handed out still resolve.

## Decisions
1. Read the printed `Deliverable:` line off the screen: **yes**, a row that starts with `Deliverable:` and carries a link, never a Links table.
2. Where a handover is kept: **filed into that agent's bench as an artifact**, in the conversation's record.
3. What the chip says: **the newest handover's name**, never a count.
4. Build slice 1 now: **yes**, and all three were built the same night.
5. (Parker's direction.) The list is **a Markdown page in the floating square**, and a plain left click on a name opens it into the square in the page's place. It needs no list code of its own: TD's Markdown view already routes a link press back to the pane.

## Slices
- [x] Slice 1: the header chip (the newest name above 470 px, `▤ artifacts` down to 264, a row in the ⋯ menu below), the history page on `▼` or Ctrl+Shift+M, and `declare_deliverable` filing an artifact into the conversation's record.
- [x] Slice 2: the once-a-second screen scan reads `Deliverable:` rows with the wrap rules below, files them the same way, and a printed handover becomes the needs-me link when nothing was declared.
- [x] Slice 3: not-opened marks kept beside the record (`opened.jsonl`) and cleared by any road that opens the file, `rewritten HH:MM` and `file gone` on a row, and the help row in all nine languages.

## What the photographs found
The chip and the page were photographed in a hidden window (special workspace, `grim -T`, keys by `send_shortcut`) with a stand-in agent painting five real handovers from the Cinema Delight agent. With every test green, the first photograph showed `the four voicings` linking to `…/sound/READ`:
- **The terminal's own wrap.** The stand-in printed before the window had grown, so the terminal wrapped the link and carried it on at column 0. The rule followed only Claude's wrap, at the message's margin.
- **A row that grew.** Once the window grew, the alternate screen was not reflowed and the row no longer reached the edge. Only the grid's wrap flag still said it carried on, so the screen rule now takes those flags beside the rows.
- **A cut link is refused.** A link that reaches the edge with nothing found to carry it is no longer kept short, because a truncated link opens nothing.
- The page carried two markers per row (the list's bullet and a dot of its own) and a 120-character shell prompt as its heading. Both fixed.

## Proven
- 28 unit tests in `handover`, 2 in `pane::handed`, and 3 pane tests through the harness. Ctrl+Shift+M opens the page and puts it away, and a link press opens the artifact in its place and clears its mark. The HC Video pane's real 100-column rows read across Claude's indented wrap and become the needs-me link, while a shell pane is not read. A real terminal's own wrap of a 103-character link is read whole.
- Six guards were each broken on purpose and each failed the test aimed at it: the margin join, the next-handover guard, the opened mark, the Ctrl+Shift+M claim, the agent gate, and the `Deliverable:` prefix.

## Not done, each a fair next request
- One list for the whole window: the same store, a second reader.
- Reading transcripts for handovers made before this build.
- A dot that relights on a rewrite. Measured against it: ten of sixteen files were rewritten after handover, mostly in batch rebuilds.
- Bench files stranded when pane ids were renumbered at a restart, which belongs to the workbench's conversation-keyed store.
- **Unmeasured:** how often a `Deliverable:` line never reaches the screen at all, a reply taller than the pane arriving in one piece. A once-a-second scan cannot see it.

## Context
- Asked 2026-10-06, from the pane titled "Artifact spin in agents": *"so I just scrolled up a bunch for a link to a file that was an artifact --- ... I would love a secondy right spin under our agents for articfact similar to what we have in workbench for ARTIFACTS... but in the main work surface"*.
- Measured the same day:
  - `~/.claude/settings.json` line 401 carries `"tui": "fullscreen"`, so Claude Code draws on the alternate screen and Terminal Delight's scrollback holds no old reply.
  - 76 Claude transcripts modified in the last seven days: 51 printed a line starting `Deliverable:` (314 lines), 32 called `declare_deliverable` (104 calls), and all 32 also printed. 19 printed and never declared.
  - One pane: the Cinema Delight agent (session 9fc192a8) handed over 16 distinct links in 26 hours, 12 declared and 4 only printed. 10 of the 16 files were modified after they were handed over, three of them (the Movement I, II and III pages) in the same minute at 09:20, and one file (`sound/README.md`, "the four voicings") no longer exists.
  - Session 1 has 69 `artifact` surfaces on disk and none under the directories of the 22 panes running then (pane ids reassigned at the 15:27 restart).
  - The HC Video pane is 100 columns wide (read off its pty) and its `Deliverable:` row is exactly 100 characters; Claude starts the continuation two spaces in, which `row_flows_into_next` does not join.
  - Ctrl+Shift+M was unbound in the pane's chord table, in `main.rs`, and in Omarchy's Hyprland bindings.
- Two rules already in the code that the chip keeps: the header's *"a number on the chrome says what it counts, or it does not go on the chrome"* (`pane.rs`, under the face toggle), and the workbench header's case that *what did this agent make* belongs in the pane (*"a rail that mixes twenty conversations is a feed"*).
