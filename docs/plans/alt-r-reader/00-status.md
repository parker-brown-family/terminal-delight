# Alt+R reads the source, at the reader's own size

**Difficulty: 7/10.** Four surfaces move together: the reader overlay's sizing, a
third seat for the document view, a second text source (the agent's transcript)
beside the grid, and the reader's keys on a document. One of them can show the
wrong agent's conversation if the binding is trusted loosely, and the reader is
opened on every kind of pane, so a wrong product call stays wrong for a long
time. That buys the four gates. If Gate 1 comes back as a bare concur, the score
was too high: collapse Gates 2–4 into one plan and say so.

**Turned out to be:** (fill in at the end)

- Gate 1 — Product: **APPROVED 2026-09-29** — the brief
  `reports/2026-09-29-alt-r-reader.html` (built by
  `reports/2026-09-29-alt-r-reader.build.py`). All five decisions concurred as
  recommended: an agent pane reads its transcript (screen as labelled fallback);
  a shell reads its scrollback re-flowed at the reader's width; the reader opens
  at the pane's own letter size (175 × 46 in his window); with a square floating
  over a terminal, Alt+R reads the square's document; typing keeps reaching the
  agent, with the pane's live input rows as a strip at the reader's foot. His
  notes: figure 04 "Great.", card A "awesome concur", figure 07 "very good
  summary - love it", decision 3 "nice!", and on figure 02: *"We have made like
  five attempts on this, and everyone has failed. We're on Rio VT now, so maybe
  this is attempt six for the win! <3"*. Nothing changed at this gate: one rubber
  stamp. If Gate 2 also comes back unchanged, collapse Gates 3–4 into one plan.
- Gate 2 — Architecture: **APPROVED 2026-09-29** — all five decisions concurred
  with no notes: `reports/2026-09-29-alt-r-reader-architecture.html`,
  `02-architecture.md`. Parker: *"Keying in on how a doc allows comments....
  Full concur … Let's send it!"* — read as: comments on a lent document must keep
  working in the reader (now an acceptance test in slice 2).
- Gates 3 and 4 — **collapsed** into `03-plan.md` (two rubber-stamp gates in a
  row: scored too high). Building from it.

## Slices (first cut, from the brief's figure 08 — Gate 4 decides)

- [ ] Slice 1 — tracer bullet: the reader has its own size (175 × 46 in Parker's
      1,576 × 950 window, at the pane's own letter size); a shell's rows keep their
      soft-wrap flag and join on it; the read stops holding the lock across the
      whole history; check Alt+R on a workbench-face pane in his window on the way
- [ ] Slice 2 — a Document-face pane lends its document view to the reader:
      Markdown, HTML, PDF, picture, video
- [ ] Slice 3 — an agent pane reads its conversation from its transcript (Claude),
      the labelled SCREEN fallback when the binding is not certain, the input strip
- [ ] Slice 4 — Codex transcripts; the floating square, if decision 4 holds

## The five attempts before this one

Parker's count, on figure 02. From `git log` and the 2026-08-31 handoff
(`handoffs/HANDOFF-2026-08-31-reader-document-model.md`). Every one of them read
the pane's grid, at the pane's width, and tried to undo that afterwards:

1. **2026-06-15** `7a43d17` — mirror the pane's styled rows, scaled to fit the
   glass on the tighter axis.
2. **2026-06-20** `ef74db9` — wrap-to-fit: re-wrap every grid row at the glass
   width, fit to height. Rows already broken at the pane's width can never get
   longer, so a narrow pane read as a thin ribbon.
3. **2026-08-31** `9fabb41` (#214) — fill the width: scale the pane's columns to
   the glass. Ends the ribbon; starts today's magnification (3.65× on a
   48-column pane).
4. **2026-08-31** `128d6cf` (#217) — the document model (`doc.rs`): heal
   width-breaks from the grid, scrollback included. Heals what the terminal
   wrapped; cannot heal what a TUI wrapped itself — and Claude indents every
   row, so it never joins a Claude reply (measured 2026-09-29).
5. **2026-08-31** `121e475` (#221) — the whole conversation, mirrored.
   **Operator decision that day: "FOCUS is a MIRROR of the screen — not a view
   of the agent's transcript — so the planned transcript source is cancelled."**
   APES ticket `build-the-agent-transcript-documentsource-reader-phase-2-mthp5tr9`
   blocked with "do not pick this up unless the mirror decision is reversed".

**Decision 1 of this Gate 1 reverses that call (2026-09-29).** Attempt six is the
one designed on 2026-08-31 and never built, plus two things none of the five had:
the reader's size stops coming from the pane, and a document pane lends its view.
Still open from then: #225 (narrow pre-resize history reads crunched).

**What rio-vt adds (checked 2026-09-29 in rio-vt 0.5.28):** `Grid<T>` is `Clone`,
and `Grid::resize(reflow: true, lines, columns)` re-wraps soft-wrapped rows,
recording an exact old-row → new-row map (`track_reflow_remap` / `ReflowRemap`).
So a shell's scrollback can be re-flowed by the terminal itself at the reader's
width, with no guessing, instead of through `Document::from_grid_rows`. It cannot
help an agent: Claude writes real newlines, which no core reflows. alacritty's
grid has the same resize-with-reflow but no row map; both cores must keep
building (`cargo test --features core-alacritty`).

## Notes for a fresh session

- **The request, 2026-09-29, in Parker's words:** the reader should open for the
  panes Ctrl+Alt+click makes (HTML, images, PDF, video, Markdown) and doesn't; on a
  workbench or terminal face it shows the terminal, never the bench ("the Workbench
  is already a processed thing"); the terminal reader stops using the terminal
  mirror; and "if I open a reader for a tiny, tiny pane, it's exactly because it's
  a tiny pane … I want the reader to have resolution … that extends far beyond the
  tiny, tiny pane." Full text is the first row of the brief's figure 02.
- **Measured:** today's reader lays out the pane's own columns and fits them to
  80% of the window (`main.rs`, the FOCUS overlay: `fit = avail_w / (cols × cell_w)`,
  clamped 0.7–6; slider `FZ_MIN 0.35 ..= FZ_MAX 1.6` × fit). On Parker's 48-column
  panes that is 3.65× with ~12 rows visible, and the slider's smallest is still
  1.28×. Cell 6.19 × 14.73 px (JetBrains Mono 14 px × text grade 0.7365).
- **Measured:** `doc::Document::from_grid_rows` never joins a Claude Code reply's
  rows, because every continuation row is indented two columns and `wrap_join`
  refuses indented rows; the whole-screen margin dedent can't fire because the ⏺
  bullets and the input box sit in column 0. Probe (real function, TextRun
  stubbed): 5 rows at 48 cols → 5 lines, widest 47, in a 150-col layout; shell
  soft-wrap → one 128-col line. Same cause as the copy-chip fix of 2026-09-28.
- **From the code:** the workbench face already mirrors the terminal on purpose
  (`mirror_snapshot` doc comment; test `the_focus_reader_mirrors_the_grid_on_both_faces`).
  Parker suspects it fails in practice — verify first in slice 1.
- **From the code:** a Document-face pane keeps the shell it was made with
  underneath, and Alt+R (a Window-layer chord on every face) mirrors that hidden
  grid. `DocumentView::set_seat` already moves one view between the square and the
  face without reopening; a `DocSeat::Reader` is the natural third seat.
- **Binding rule:** use `paneident::certain` only; an unbound pane gets the SCREEN,
  labelled, never a directory-wide guess (the 2026-09-18 incident in paneident.rs).
- Code was read at origin/main `5ba60ed`; the primary checkout sat at detached
  `53f476d`, 17 commits behind. Build from a fresh worktree off main.
- APES ticket: `gate-1-brief-what-alt-r-opens-on-every-pane-at-the-reader-s-own-size-mumyre45`.
