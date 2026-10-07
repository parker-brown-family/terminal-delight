# Plan: the artifact chip

**Difficulty 5/10.** See `00-status.md`. The page Parker reads is `reports/2026-10-06-artifact-spine.html`; this file is the same plan in the terms a slice is built from. Redrawn per pane on 2026-10-06; the window-wide list it replaced is described at the end.

## What it is for

An agent hands over a report, a page or a video by naming it at the end of a turn. Today that name lives in two places nobody can browse: the pane's one latest declared deliverable, which the needs-me queue offers and a restart forgets, and the agent's own reply, which on the alternate screen is not in Terminal Delight's scrollback at all. Finding an earlier link means scrolling inside Claude Code. One agent handed over 16 links in 26 hours on 5–6 October. This puts that list in the pane it belongs to.

## Product contract

1. **The chip.** Every agent pane's header carries a chip directly after the face toggle, holding the name of the newest handover in that pane, capped at about 24 characters with an ellipsis, then a `▾`. A pane whose agent has handed nothing over draws no chip. A shell pane never has one.
2. **Not opened.** A dot before the name, and the chip in the accent, while the newest handover has not been opened from the chip, the list, the square or the needs-me row. Plain once it has. The chip never shows a count: the header's rule is that a number on the chrome says what it counts.
3. **Narrow panes.** Below about 470 px the name gives way to `▤ artifacts ▾`. Below 264 px the chip goes into the ⋯ overflow, as the header's other controls do, listed as `Artifacts`.
4. **Click.** The name opens the newest handover over the pane (the floating square, as Alt+click does; a kind the square cannot draw goes to the desktop). The `▾`, or Ctrl+Shift+M with the pane focused, opens the list under the chip.
5. **The list.** Header `HANDED OVER · n` and the keys. Rows newest first by when each was handed over: not-opened dot, kind (`html`, `md`, `mp4`, `wav`, `pdf`, `link`), label, age by `attention::age_label`; second line `declared` or `said`, then `rewritten HH:MM` when the file changed after the handover, or `file gone` when it no longer exists.
6. **Keys.** ↑↓ or j/k and 1–9 move; ↵ opens over the pane; `b` opens beside it in a split; esc folds. Reading the list never moves keyboard focus into the terminal.
7. **On the bench face** the chip selects the ARTIFACTS shelf instead of opening a dropdown, because that shelf is the same list.

## Where rows come from

One store, two new writers, one reader per pane.

- **The store** is the agent's own bench: an `artifact` surface in `surfaces/<session>/<pane>/`. The bench re-reads that directory when a window opens, and its ARTIFACTS shelf already draws it, so the chip and the shelf cannot disagree.
- **Writer 1, the verb.** `declare_deliverable` keeps setting the pane's in-memory `Deliverable` for the needs-me row, unchanged, and also writes `handover-<hash of href>.json` into the pane's drop box: `kind: artifact`, `title` the label, `model.href`, and notes carrying `source: declared` and the handover time.
- **Writer 2, the screen.** The two clocks that keep what the person last asked (`latch_asked`: the tail at 120 ms, the whole screen at one hertz) also look for a handover row and file it the same way with `source: said`. When the turn declared nothing, a printed handover also becomes the pane's `Deliverable`, so the needs-me row's `o` reaches the 19 in 51 agents that only print.
- **The reader** is the pane's own header: it walks that pane's `Shelf::Artifacts`, keeps surfaces carrying an `href`, de-duplicates on the href, and orders by handover time. A pure function, `artifacts::collect`, with table tests; the header only draws what it returns.

**One file per link per agent.** The id is derived from the href, so the same link seen on twenty turns overwrites one file and keeps its first handover time. If the bench already holds an artifact from the agent itself with the same href, Terminal Delight writes nothing.

## The handover row rule (slice 2)

A row is a handover when, after its leading spaces, it starts with `Deliverable:` and a `file://`, `http://` or `https://` link appears on it or on the next row. Specimens, all read off panes open on 2026-10-06:

| Screen | Result |
|---|---|
| `  Deliverable: The File Drop —` then `  file:///home/parker/Work/terminal-delight/reports/2026-10-06-the-file-drop.html` | one row, link on the next row |
| `  Deliverable: Club HC VIDEO spec — file:///…/hc-vid` (100 characters in a 100-column pane) then `  eo-web/docs/plans/club-hc-video/2026-09-29-club-hc-video.html` | one row, after a margin-aware join |
| `  Deliverable: line, that's where the actual artifacts live. I'll review` | refused, no link |
| `  ⎿  "Deliverable:"` | refused, the row does not start with it |
| `  │ file:///…/td-movements-symphony-final-flat.mp4  │ The symphony, flat  │` | refused, a Links table row |

**The margin-aware join.** `row_flows_into_next` joins a row to the next only when the next one starts with a non-space character. Claude indents its wrapped rows to the message margin, so the join here is: the row reaches the pane's last column, and the next row starts at the same margin as the handover row with a token that continues the link. The existing function stays as it is; the copy chip and the link click depend on it.

The label is the text between `Deliverable:` and the dash before the link, trimmed; with no label, `mcp::deliverable_fallback_label` names it, as the verb does.

## Not-opened marks (slice 3)

A row is not opened until it is opened from the chip, the list, the square, beside, or the needs-me row. Once opened it stays opened. A rewrite of the file does **not** relight it: in the measured pane, 10 of 16 files were modified after their handover, three in the same minute, which reads as rebuilds and not revisions. The row shows `rewritten HH:MM` instead, as a fact. Marks are kept in the session's saved state so a restart does not light every row.

## Unknown is not zero

- An age that cannot be read is a dash, never `0s`.
- A file that no longer exists says `file gone`; a file whose time cannot be read says nothing about rewriting, rather than claiming it was not rewritten.
- A pane whose bench directory cannot be read draws `▤ unreadable` instead of no chip, because no chip means *nothing handed over*.
- Before the first walk of the shelf finishes, the chip is absent and the list says it is still reading, which is different from *Nothing handed over yet.*

## Slices

1. **The chip.** The header chip, its narrow and tucked forms, the list and its keys, Ctrl+Shift+M; `artifacts::collect` with table tests; the verb files into the bench. The `workbench.rs` header gains one line saying the chip is the terminal face's door to the ARTIFACTS shelf.
2. **The printed line.** The handover-row rule and the margin-aware join, with the five specimens above as tests; filing from both clocks; a printed handover filling the needs-me link when nothing was declared. A counter in the window log of sightings, to compare against the transcripts later.
3. **Marks and words.** The not-opened dot and its persistence, `rewritten` and `file gone`, help rows in all nine locales (`lang.rs`).

Every guard gets a mutation that fails it, per the repository's habit.

## Out of version one

One list for the whole window (the first draft): the same store read by a second reader, if a cross-pane view is wanted later. A dot that relights on a rewrite. Links tables and every other printed link; reading agent transcripts; bench files stranded when pane ids changed at a restart, which belongs to the workbench's conversation-keyed store; deleting or renaming from the list; Codex panes are covered by the screen rule unchanged and were not measured.
