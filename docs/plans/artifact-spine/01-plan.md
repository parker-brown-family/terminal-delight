# Plan: the artifact chip, as built

**Difficulty 5/10.** See `00-status.md`. The page Parker approved is `reports/2026-10-06-artifact-spine.html`; this file is the plan as it was built, in the terms the code uses. Where the build departed from the page, the departure is said here.

## What it is for

An agent hands over a report, a page or a video by naming it at the end of a turn. Before this, that name lived in two places nobody could browse: the pane's one latest declared deliverable, which the needs-me queue offered and a restart forgot, and the agent's own reply, which on the alternate screen is not in Terminal Delight's scrollback at all. Finding an earlier link meant scrolling inside Claude Code. One agent handed over 16 links in 26 hours on 5–6 October. This puts that list in the pane it belongs to.

## Product contract

1. **The chip.** Every pane whose agent has handed something over carries a chip directly after the face toggle, holding the newest handover's name, cut at 28 characters with an ellipsis, then a `▼`. A pane whose agent has handed nothing over draws no chip. The chip never shows a count: the header's rule is that a number on the chrome says what it counts.
2. **Not opened.** While the newest handover has not been opened, a dot leads the name and the chip wears the accent. Once opened, it is plain.
3. **Narrow panes.** Below 470 px the name gives way to `▤ artifacts` (the word is localised). Below 264 px the chip goes into the ⋯ overflow as a row of its own, as the header's other controls do.
4. **Click.** The name opens the newest handover in the floating square over the pane; a kind the square cannot draw goes to the desktop, and a file that is gone says so on the pane. The `▼`, or Ctrl+Shift+M with the pane focused, opens the history page in the same square, and the same chord on the page puts it away.
5. **The history page** is Markdown, written to `$XDG_STATE_HOME/terminal-delight/handovers/<conversation>/handed-over.md` (or `pane-<session>-<id>/` for a pane not yet bound to one). It is headed with the pane's name, gives the count and how many are new, and lists every handover newest first under day headings (Today, Yesterday, then dated). Each row is the name as a link, and under it `new` when not yet opened, the kind (`html`, `md`, `mp4`, `wav`, `web`), the time it was handed over, how (`declared`, `presented`, `said`), and `rewritten HH:MM` or `file gone` where true. **A plain left click on a name opens it into the square in the page's place** — Parker's direction, and the gesture TD's Markdown view already routes for any link.
6. **Order** is by when each was handed over, not by file time: ten of the sixteen measured files were rewritten afterwards, three in one minute, and a list ordered by file time reshuffles itself on every rebuild.

**Departed from the page:** the page proposed a dropdown under the chip with ↑↓, ↵ and `b` for beside. Parker's direction put the list in the floating square as a Markdown file instead, which gives it the square's own zoom, split, desktop and Esc, and makes it a file an agent can read. Keyboard movement through rows was not built: the square scrolls with the wheel, and a row opens with a click.

## Where rows come from

One store, two new writers, one reader per pane.

- **The store** is the conversation's record (`conversations/<root>.ws`). A handover is filed there as an ordinary `artifact` surface with id `handover-<FNV-1a of the file>`, plus a `handover` key saying how it arrived, so the bench's ARTIFACTS shelf shows it with no code of its own.
- **Writer 1, the verb.** `declare_deliverable` still sets the pane's in-memory `Deliverable` for the needs-me row, and now also calls `hand_over(…, Declared)`.
- **Writer 2, the screen.** The one-hertz walk of every agent pane (beside `latch_asked(None)` in `main.rs`) calls `scan_handovers`, which reads the live screen's rows and wrap flags in one lock and files each new handover as `Said`. A new one also becomes the pane's needs-me `Deliverable` when the agent did not declare that same file.
- **Also kept:** an artifact the agent presents on its own bench is recorded as `Presented` as it lands (`present` → `handover_presented`); it is already filed, so nothing is written.
- **The reader** is the pane's own ledger (`self.handovers`), rebuilt from the conversation's whole record when the pane is bound to a conversation (`bench_adopt_conversation` → `handovers_from_record`), not from the bench's 64 surfaces, because a response takes one of those every turn.

**One row per file.** `key_of` reduces every spelling of a file to its path, so `/a/b.html`, `file:///a/b.html` and a percent-encoded copy are one handover. The same file handed over again keeps its first time; a line read off the screen that the agent then declares is upgraded in place and refiled. If the bench already holds an artifact the agent itself presented for that file, TD files nothing.

## The screen rule

A row is a handover when, after its margin and any bullet the renderer drew, it starts with `Deliverable:` and a link follows: on that row, carried across the rows the link was wrapped onto, or as the first thing on the next non-blank row (the house format puts the label and a dash on one line and the URL on the next). A link is a `file://` URL or absolute path with a real path, or an `http(s)` address with a host, so the template agents are taught, `<file:// or https:// URL>`, is not one.

**Carrying a link across a wrap** (`carry`):
- the terminal's own wrap: the grid flags the row (`WRAPLINE`, read off any cell, because a row written narrow keeps its flag after the pane grows and the row stops reaching the edge), and the next row is joined at any margin;
- Claude Code's own wrap, which sets no flag: the row reaches the last column and the next row starts at the message's margin, or at column 0;
- never onto a blank row or a row that opens a handover of its own: there the link ended exactly at the edge.

**A cut link is refused.** A link that reaches the edge with nowhere found to carry it is not recorded at all, because a truncated link opens nothing.

Specimens, each a real row from a pane on 2026-10-06, are the unit tests' fixtures: the house format (`The File Drop`), Claude's indented wrap at 100 columns (`Club HC VIDEO spec`), the terminal's column-0 wrap and the row that grew (`the four voicings`, found by the photographs), and the refused ones — prose wrapped onto a row start, a tool call echoing the word, a Links table row, the template.

## Not-opened marks

A handover is opened when its file is opened in this pane by any road: the chip, the page, an Alt+click on its path, the needs-me queue (`open_float`, the page's link route, and a desktop hand-off all call `note_opened_path`). Once opened it stays opened; a rewrite afterwards is shown as a fact on the row and does not relight it. Marks are appended to `handovers/<conversation>/opened.jsonl` and read back when the pane is bound, so a restart does not light every row.

## Unknown is not zero

- A handover's time that the C library cannot place says `time unavailable` under its own heading, never 1970.
- A file that is not there says `file gone`; a file whose state could not be read, or a web page, says nothing about rewriting rather than claiming it was unchanged.
- A pane with nothing handed over draws no chip; its page says how things arrive rather than drawing an empty list.
- Under test, the feature's directory is a folder of the test process's own, so a test run leaves nothing in the state directory of the person running it.

## Out of version one

One list for the whole window (the first draft): the same store read by a second reader. A dot that relights on a rewrite. Links tables and every other printed link. Reading agent transcripts for handovers made before this build. Bench files stranded when pane ids changed at a restart, which belongs to the workbench's conversation-keyed store. Deleting or renaming from the list. Codex panes are covered by the screen rule unchanged and were not measured.
