# Plan: program design and slices, as one page

**Gates 3 and 4 are collapsed into this page.** Gate 1 and Gate 2 both came back
as full concurs with nothing changed — two rubber stamps in a row, which says the
7/10 was scored too high for the ceremony it bought. Parker, approving Gate 2:
*"Full concur … Let's send it!"* So this is one plan, and the build starts from
it. Every slice still ends green on the CI gates (fmt, clippy `-D warnings` on
both cores, the suite on both cores) and is proved in a running window.

Branch `feat/alt-r-reader` in `~/Work/td-alt-r-reader`, off main at `5ba60ed`.

## Slice 1 — the reader has its own size (tracer)

- `main.rs`, the FOCUS overlay: a flat read's letter scale is `focus_zoom` alone
  (1.0 = the pane's own letters) instead of `fit × focus_zoom`; the text column is
  the glass's full width (`body_w = avail_w`). Crawl keeps its fit. Ctrl+wheel over
  the reader steps `focus_zoom` instead of the pane's text dial.
- Pure helpers, tested: `reader_scale(crawl, fit, zoom) -> f32` and
  `focus_zoom_step(zoom, notches) -> f32`. Parker's window gives 175 columns at 1.0.
- Moved to slice 3: the header naming its source. The reader's header strings
  are literals today ("FOCUS", "esc to close"); naming the source properly
  means nine translations in `lang.rs`, and it matters most beside the
  transcript's own chip.
- **Proof:** a 48-column pane in a rig window opens at the pane's own letters with
  ~175 columns; Alt+R on a workbench-face pane reads that pane's terminal.

## Slice 1b — a shell joins on its own flag; the read stops holding the lock

- `vt/`: `Term::lines_evicted() -> Option<u64>` (rio: `Some`, alacritty: `None`).
- `doc.rs`: `GridRow { line: DocLine, wrapped: bool }` and
  `Document::from_wrapped_rows(&[GridRow])` — a flagged row keeps its trailing
  spaces and continues on the next row; every other row ends its line. No guess.
- `pane.rs`: `grid_rows_in` keeps each row's WRAPLINE flag and drops the
  `LEADING_WIDE_CHAR_SPACER` cell. Shells build with `from_wrapped_rows`; an agent
  pane's screen keeps `from_grid_rows` (its rows carry no flags) until slice 3.
  History rows are cached; each rebuild reads only rows new since the last one
  (counted from `history_size + lines_evicted`) plus the screen, and rebuilds in
  full on a resize, a clear, or when the count is unknown (alacritty with a full
  history).
- **Tests:** the real `git log` sample joins to three lines; `ls -C` output stays
  as printed; a space at the wrap point survives; appending k lines reads only
  k + screen rows; eviction drops rows from the front; a resize rebuilds.

## Slice 2 — a document is lent to the reader

- `docopen.rs`: `ReadFrom { Grid, Document(DocSeat), Transcript }` and a pure
  `read_from(face, square_up, transcript_bond)`; a test walks every combination.
- `docview.rs`: `lend(reading)` resets the measured frame and lets `key()` treat
  the lent view as the face's; `page(dir)` pages by its own height.
  `backend.rs`: `reseated()`, which Markdown uses to re-anchor to its top block.
- `pane.rs` / `pane/doc.rs`: `lent: Option<DocSeat>`, `set_being_read(on, cx)`
  lends and takes back, `lent_view()`. The face and the square draw "being read"
  where they draw the view today. The flag also clears in `show_document` and
  `float_of`.
- `main.rs`: the overlay draws `lent_view()` in its body, asked every frame and
  never held; pointer events go to the view through `unwarp`; `reader_key` offers
  Esc to the view first and sends the paging keys to `page`.
- **Comments keep working in the reader** (Parker, approving Gate 2: *"Keying in
  on how a doc allows comments"*): a note, a CONCUR stamp and ↪ send all work on a
  brief read through Alt+R, and ↪ lands where it lands from the pane.
- **Proof:** each of the five kinds opened through Alt+R in a rig window,
  photographed; a note and a concur saved from inside the reader, read back.

## Slice 3 — an agent reads its conversation

- New `transcript.rs`: `Tail` (byte offset, partial line, reset on truncation or a
  new path), `Conversation` / `Entry` / `EntryKind`, `fold` for Claude records
  (a reply's blocks joined by message id; a tool call and its result on one line;
  thinking, attachments, modes, file history and queue records skipped; compaction
  a divider; images a placeholder; spilled output by link).
- `TranscriptSource: DocumentSource`: comrak into styled lines — headings bold,
  tables as aligned text, code as blocks — laid out by `doc::layout`.
- `toolprop.rs`: `ToolProbe::path()`. The overlay reads through it only when the
  bond is certain; otherwise the screen, labelled "transcript not bound".
- The live footer: the pane's rows from the input box up.
- The header names its source in all nine languages: TRANSCRIPT, SCREEN (with
  "transcript not bound"), SCROLLBACK, and ALTERNATE SCREEN for vim and friends.
- **Tests:** fold fixtures cut from real transcripts; a tail that reads only new
  bytes across a partial line; a table laid out as aligned text.

## Slice 4 — the floating square, and Codex

- `read_from` with a square up → `Document(Float)`.
- Codex rollouts (`response_item` stream, `call_id` pairs) fold into the same
  `Conversation`.

## Least confident decisions

1. The slider's range becomes 0.5–2.5 × the pane's letters (it was 0.35–1.6 × fit).
2. Ctrl+wheel over the reader zooms the reader, no longer the pane behind it.
3. An agent pane's screen keeps the old width guess until slice 3 replaces it.
4. The history cache trusts `history_size + lines_evicted` as a row count; if a
   core ever changes history without moving it, the cache serves stale rows.
