# Architecture: Alt+R reads the source, at the reader's own size

**Gate 2, drafted 2026-09-29, waiting on Parker.** The drawn version, with the
five decisions, is `reports/2026-09-29-alt-r-reader-architecture.html` (built by
the `.build.py` beside it). This file is the text record for a fresh session.
Researched read-only against main at `5ba60ed` in the worktree
`~/Work/td-alt-r-reader`; the claims marked *checked* were confirmed by hand.

## Fit

- **The reader overlay** (`main.rs`: `open_focus_read`, `close_focus_read`, the
  `focus_overlay` block, `FocusMap`). The glass already sizes from the window
  (80%). Only the letter size came from the pane (`fit = avail_w / (cols ×
  cell_w)`); it goes. Text is laid out at the reader's own width — `avail_w /
  cell_w` at the pane's own letter size, 175 × 46 in Parker's window — with the
  slider and Ctrl+wheel scaling from 1.0. When the source is a document, the body
  hosts the lent view instead of `FocusMap` rows.
- **Which source** — one pure table beside `mirror_snapshot` (`pane.rs`), in the
  style of `keylayer.rs`: `read_from(face, square_up, transcript) -> ReadFrom {
  Grid, Document(DocSeat), Transcript }`. It replaces the "grid on every face"
  rule in `mirror_snapshot`'s doc comment, and the test
  `the_focus_reader_mirrors_the_grid_on_both_faces` changes in the same commit.
- **A shell** — `grid_rows_in` (`pane.rs:5107`, *checked*) holds the terminal lock
  across the whole range and keeps only each character and its colour. It will
  keep each row's WRAPLINE flag (both cores export it: `vt/rio.rs`,
  `vt/alacritty.rs`) and read through `for_each_row` rather than one long lock.
  `doc.rs` joins on the flag, exactly; the width guess in `from_grid_rows` stays
  only for rows with no flag. Joined history is cached and each update re-reads
  the seam and the screen; rows falling off the top come from rio-vt's eviction
  count, `Option<u64>` at the boundary because alacritty has none (then: full
  rebuild). Alternate screen: drawn as its screen, no history, labelled.
  **Not used:** cloning the grid and `Grid::resize(reflow)`. It works on both
  cores (*checked*: `Grid: Clone`, reflowing `resize`; only rio-vt records a
  `ReflowRemap`) but costs a 16–58 MB copy and a wider read for the same joins.
- **An agent** — a new module: a conversation model and a tail that reads only
  new bytes. None of the five existing transcript walkers (`mcp_tail`,
  `derive::walk`, `vitals::parse_transcript`, `notify`, `toolprop`) keeps prose in
  order, and each re-reads a 256 KiB tail that one line can fill (p99 line 519 KB,
  largest 1.1 MB). Markdown goes through comrak (already a dependency, *checked*)
  into styled lines — headings bold, tables as aligned text — and then
  `doc::layout`, keeping virtualised rows, selection and copy. The binding is
  `Workspace.tool_probe` (`main.rs:4160`, *checked*): add `ToolProbe::path()`
  (`toolprop.rs:299`, *checked*) — the certain path or none, refreshed every 2 s,
  no I/O at open. Re-ask it each poll; reset when the path changes or the file
  shrinks. A live footer draws the pane's rows from the input box up.
- **A document** — lent, never moved. The pane keeps ownership (`doc`/`float`),
  so the Document-face invariant, the saved layout, router and MCP lookups, links
  and the ↪ target all stay put. The face and the square draw "being read" where
  they draw `.child(view)` today; the Workspace asks `lent_view()` every frame and
  keeps no handle. **Not** a third `DocSeat`: that enum already names which of the
  pane's two documents something came from (`SendNotesBeside.seat`, `doc_said`,
  `follow_doc_link`, `notes_send_target`). Markdown has to re-anchor to its top
  block when lent and returned (`Backend::reseated`), or it loses its place.
- **Keys** (`keylayer.rs`, the Reader rung; `pane.rs:6176` `reader_key`, *checked*:
  it pages or closes, nothing else). With a document lent, Esc goes to the view
  first (an open note box closes before the reader) and the paging keys go to the
  view, paging by its own measured height.

## Endpoints

None. No MCP verb, ctl command or wire format changes.

## Data

Nothing stored changes. In memory only: a joined-history cache per pane being
read, one `Conversation` per open reader, and a `lent` flag on the pane.

## Flow

```
Alt+R → Workspace::open_focus_read(pane) → pane.set_being_read(true, cx)
each frame → read_from(face, square_up, tool_probe bond)
   Document   → draw pane.lent_view() in the glass (pointer unwarped, frame reset)
   Transcript → doc::layout(TranscriptSource(conv), reader cols) + live footer
   Grid       → doc::layout(joined grid document, reader cols) (+ SCREEN label if an agent)
each second (transcript) → Tail::poll → Conversation.rev++ → layout memo keyed (rev, cols)
Esc → view.key first (note box) → close_focus_read → pane.set_being_read(false, cx)
```

## External

Claude Code transcripts under `~/.claude/projects/<slug>/<session>.jsonl`, with
subagents in `<session>/subagents/` and spilled tool output in
`<session>/tool-results/`. Codex rollouts (`~/.codex/sessions/**/rollout-*.jsonl`)
need their own reader: a later slice.

## Decisions waiting (the brief's grill)

1. Join a shell's lines on the terminal's flag, with no copy of the grid.
2. Draw a conversation in the reader's own text layout, not the Markdown view.
3. Show a reply when each block of it is finished; the footer carries the rest.
4. Keep what the census keeps: prompts and replies, one line per tool call with
   output behind a click, thinking hidden, the rest skipped.
5. Lend documents, and keep a picture at no more than its own pixels until zoomed.

## Risks

- A reply is invisible to the transcript until its block completes; a tool call
  awaiting permission looks the same as a running one (the footer covers both).
- Launch-time bindings count as certain but pick the closest of near-simultaneous
  agents; after `/clear` a pane can briefly be bound to its own previous file.
- HTML renders once more on lend (soft for ~250 ms) and possibly on return; PDF
  redraws tiles; video decodes at up to its own pixels (CPU unmeasured).
- Replica repair skips `close_focus_read`, so the lent flag must also clear in
  `show_document` and `float_of`. alt+k, `ctl doc close` or MCP can change the
  source mid-read; asking `lent_view()` every frame absorbs it.
- `set_being_read` gains `cx` at four call sites, one of them the `TD_FOCUS_DEMO`
  hook that bypasses `open_focus_read`.
