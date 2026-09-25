# Plan: Brief Beside

_One combined plan (difficulty 5/10): product, architecture and slices. The page for Parker is `reports/2026-09-24-brief-beside.html`._

## Product

An agent writes a brief; you read it beside the agent and note it; the agent gets your notes. Today the last step is "read my notes in <file>", and the agent re-reads a 1,500-word file to find 80 words of notes. Two changes close the loop:

1. **The agent gets the notes alone.** Either you press ↪ in the brief's notes bar and the notes map lands in the agent's prompt, or the agent asks for it itself. The map is `buildMap`'s text: anchor, heading, notes. It is the same text "copy map" gives.
2. **The brief appears beside the agent without a click.** When an agent finishes a brief, it opens it in a pane beside itself, and the attention rail's deliverable row gains "open beside" for any you missed.

Out of scope: notes flowing without a person pressing anything (TD never types on its own initiative), and any change to how briefs are written.

## Architecture

**Slice 1, `document_notes` (MCP, read-only).** Caller-scoped the way `leave_note` is: it resolves the calling agent's pane, then the document "beside" it. That is, in order, a float over that pane, else a document pane in the same tab (the split it opened), else none. It answers `{ path, notes, concurs, unsaved, map }`, the same fields `ctl doc notes` returns today, with `map` the `buildMap` text. It writes nothing, anywhere. `docs/security/terminal-input.md` needs no row: MCP verbs still write to no pty.

**Slice 2, ↪ send to agent (a gesture).** A new button in the notes bar, shown when the brief sits beside an agent pane (the float's own pane, or the split's source pane). A press calls `paste_text(map)` on that pane: bracketed paste, no Enter, exactly what paste (#6) does with the clipboard. The person reads it in the prompt and presses Enter. This adds row #9, "`send_notes` — a press on ↪ in a brief's notes bar — pointer", and moves the manifest's `notifier.notify` count from 9 to 10, in the same pull request. With unsaved notes the button sends what is on screen and says so. It does not save first, because saving is its own decision.

**Slice 3, `open_document` (MCP) and "open beside" on the rail.** `open_document { path, placement: "beside" | "here" }`, caller-scoped. It opens only in the caller's own pane or tab, and never names another pane. It goes through the same router as the click: the four-pane cap falls back to the float, and a file already open gets focus. The AGENTS.md "Deliverable:" rule gains one line: after declaring an HTML or Markdown deliverable, open it beside you. The rail's deliverable row gets an "open beside" action for the pane that declared it.

## Tests (each fails before its slice)

- `document_notes`: answers the float's map; the split's map; none when nothing is beside the caller; never another tab's document; the map is byte-identical to `ctl doc notes`' map on the shared fixtures.
- ↪: pastes with bracketed-paste markers when the program asked for them, and sends no `\r`; is absent when no agent pane is beside the brief; the manifest's count check moves to 10, and a scan finds the new site only inside `send_notes`.
- `open_document`: refuses a relative path, a path TD cannot draw, and a pane that is not the caller's; lands beside the caller; floats at four panes; focuses an already-open file.

## Least confident

1. **Which pane counts as "beside".** A float's own pane is clear. A split's source pane is clear until the person moves panes around; the rule falls back to "the agent pane in the same tab, nearest", and says so when there is more than one.
2. **Agents opening splits on their own.** Scoped to the caller's own tab and capped at four panes, but it still changes the layout while you may be looking at another pane. The alternative is the rail button alone.
3. **Paste without Enter** keeps the person in the loop and costs one keypress. The alternative, sending with Enter, would make TD type into a live conversation, which the manifest forbids.
