# Status: the artifact chip

**Difficulty: 5/10.** A chip in every pane's header over machinery that already exists: the bench store, the floating square, the screen scans that keep what you asked. The expensive call is filing every handover into the agent's bench, because once agents' shelves carry them, changing where they live is a migration. That buys **one combined plan page and one approval.**
**Turned out to be:** _filled in at the end_

- Plan (product, architecture, slices in one): **awaiting approval**, through notes on `reports/2026-10-06-artifact-spine.html` (titled *Artifact Chip*). `01-plan.md` is the proposal.
- **Redrawn per pane, 2026-10-06.** The first draft was one list for the whole window, a second pill under the needs-me count. Parker: *"What if the chip is PER PANE (not tab not project etc...) rather than global?"* The first draft's case for one list was that he went looking without knowing which pane held the link; he had in fact scrolled up inside one pane, so that premise was wrong. Per pane also follows the workbench header's own argument instead of amending it. The folder keeps its `artifact-spine` slug so the links already handed out still resolve.

## Decisions asked
1. Read the printed `Deliverable:` line off the screen? Recommended: yes, a row that starts with `Deliverable:` and carries a link, never a Links table.
2. Where is a handover kept? Recommended: filed into that agent's bench as an artifact.
3. What does the chip say? Recommended: the newest handover's name, not a count.
4. Build slice 1 now? Recommended: yes.

## Slices
- [ ] Slice 1: the header chip, its list and keys, the narrow and tucked forms; `declare_deliverable` also files an artifact into the pane's bench.
- [ ] Slice 2: the screen scans read a `Deliverable:` row and its link, with a margin-aware join, and file it the same way; a printed handover also fills the needs-me row's link.
- [ ] Slice 3: the not-opened dot kept across a restart, the rewritten and file-gone facts on a row, help rows in nine languages.

## Context
- Asked 2026-10-06, from the pane titled "Artifact spin in agents": *"so I just scrolled up a bunch for a link to a file that was an artifact --- ... I would love a secondy right spin under our agents for articfact similar to what we have in workbench for ARTIFACTS... but in the main work surface"*, with a screenshot of the open needs-me queue reading "Nothing is waiting on you." Then: *"What if the chip is PER PANE (not tab not project etc...) rather than global?"*
- Measured the same day:
  - `~/.claude/settings.json` line 401 carries `"tui": "fullscreen"`, so Claude Code draws on the alternate screen and Terminal Delight's scrollback holds no old reply. TD's grep found `Deliverable:` only on rows near the bottom of each pane, and did not find this pane's own session-start line, which was on screen in the screenshot.
  - 76 Claude transcripts modified in the last seven days: 51 printed a line starting `Deliverable:` (314 lines), 32 called `declare_deliverable` (104 calls), and all 32 also printed. 19 printed and never declared.
  - One pane: the Cinema Delight agent (session 9fc192a8) handed over 16 distinct links in 26 hours, 12 declared and 4 only printed. 10 of the 16 files were modified after they were handed over, three of them (Movement I, II and III pages) in the same minute at 09:20, and one file (`sound/README.md`, "the four voicings") no longer exists.
  - Session 1 has 69 `artifact` surfaces on disk and none under the directories of the 22 panes running now (pane ids 1 to 22, reassigned at the 15:27 restart).
  - The HC Video pane is 100 columns wide (read off its pty) and its `Deliverable:` row is exactly 100 characters; Claude starts the continuation two spaces in, which `row_flows_into_next` does not join.
  - Ctrl+Shift+M is unbound in the pane's chord table, in `main.rs`, and in Omarchy's Hyprland bindings.
- Two rules already in the code that the chip keeps: the header's *"a number on the chrome says what it counts, or it does not go on the chrome"* (`pane.rs`, under the face toggle, from the day a bare `2` beside BENCH was removed), and the workbench header's case that *what did this agent make* belongs in the pane (*"a rail that mixes twenty conversations is a feed"*).
