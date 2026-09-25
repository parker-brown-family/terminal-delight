# Status: Brief Beside — a document and its agent, talking

**Difficulty: 5/10.** It opens a new way for text to reach an agent's prompt, and a wrong call there is a security review redone. Everything it needs exists already: the notes layer, the split, the MCP server, paste. That buys **one combined plan page and one approval.**
**Turned out to be:** _filled in at the end_

- Plan (product, architecture, slices in one): **APPROVED 2026-09-25**. Parker: "Grea t- you go ahead and FIX EVERYHITNG!", in reply to "Approve Brief Beside, or what should change?". Nothing changed, so the three recommendations stand: ↪ pastes without Enter, agents open their own brief beside themselves in their own tab only, and the slices run in the order written. `01-plan.md`; page `reports/2026-09-24-brief-beside.html`.

## Slices
- [x] Slice 1: `document_notes`, a read-only MCP verb that answers the notes map of the document beside the caller
- [ ] Slice 2: ↪ send to agent, which pastes the notes map into the agent's prompt without pressing Enter (row #9 of the terminal-input manifest)
- [ ] Slice 3: `open_document`, an MCP verb for an agent to open its own deliverable beside itself, plus "open beside" on the rail's deliverable row

## Notes for a fresh session
- Parker agreed to both ideas on 2026-09-24: *"re-reading a 1500+ word file seems inefficient… I have buy in - agree"* and *"scanning through docs is a bit clunky… agree"*.
- In the same message, the brief-length rule: *"having the brief is meant to SPEED UP the gist"*. The plan page is measured with `~/.claude/writing/tools/brief-length.py`.
- `docs/security/terminal-input.md` governs every write to a terminal. TD never types into a running terminal on its own initiative. The new write in slice 2 is a person's gesture, shaped like paste (#6), and adds row #9.
