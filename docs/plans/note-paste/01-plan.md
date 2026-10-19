# Pasting into a note: plan

Approved 2026-09-29 through notes on `reports/2026-09-29-pasting-into-notes.html`. That brief proposed putting a picture into a note's words; Parker changed it to a picture attached to the element, listed and never drawn. This page is the plan as approved.

## What a person gets

A note box is open on a brief or a Markdown file, in the floating square or on the Document face.

- **Words.** Ctrl+V, Ctrl+Shift+V or Shift+Insert pastes the clipboard's text into the note being written, at the caret and over any selection, with its line breaks. Omarchy's Super+V and its clipboard manager (Super+Ctrl+V) arrive as those chords, so they paste too. A paste that would take a note past 20,000 characters is cut there, and the bar says how much went in.
- **A picture.** A copied picture (a screenshot, an image copied from a browser, one picked from Omarchy's clipboard manager) pasted while an element's note box is open is attached to that element at once. The box lists the element's pictures in a section under its notes as `[doc-image #1]`, `[doc-image #2]`, with a ✕ each, and never shows where the file is. To talk about one, the person writes its label in a note. Nothing is drawn in the document and nothing opens on a click: this is a one-way pass from a screenshot to the agent.
- **The agent.** The map lists each picture under its element with its full path, so the agent opens it with its Read tool:

```
[fig-09-inside-and-outside] 09 · Inside and outside the plan
  · the fence needs a third column
  · [doc-image #1] /home/parker/.local/state/terminal-delight/notes/pictures/2026-09-29-pasting-into-notes-html-1fa785be139dad14/doc-image-1.png
```

## Today

- **Ctrl+V does nothing.** The key reaches the note box, and `EditBuffer::apply` (main.rs) drops every Ctrl chord it does not name.
- **Ctrl+Shift+V goes to the terminal.** `keylayer::pane_chord` claims it, and the pane's chords sit above the square on the ladder (`LADDER`), so the note box never sees it. `paste_clipboard` then pastes into the terminal under the square, which on an agent's pane is its prompt. On the Document face it refuses, and nothing happens.

Both read from the code, not reproduced in a window; slice 1's pane tests reproduce them. The second is a gap in a rule rather than a decision: the ladder's own test says a note being written takes every key *"and not to the shell under the square, while the window's and the pane's chords still work"* (`a_note_being_written_in_a_float_takes_every_key_but_the_chords`). The pane's chords were kept for closing the tab, finding and opening panels. Ctrl+Shift+V is the one among them that writes into that shell.

## Omarchy first

Read from `/usr/share/omarchy/default/hypr/bindings/clipboard.lua` and the clipboard manager's scripts in `/usr/share/omarchy/bin/`:

| Gesture | What the window receives |
|---|---|
| Super+V over a window Omarchy tags as a terminal | Shift+Insert |
| Super+V over any other window, TD included (its app id `terminal-delight` is not in Omarchy's terminal pattern) | Ctrl+V |
| Super+Ctrl+V, then a text entry | `wl-copy` of the text, then Shift+Insert typed by `wtype` |
| Super+Ctrl+V, then an image entry | `wl-copy --type <mime>` of the file, then Shift+Insert typed by `wtype` |

`workbench::is_paste_chord` already names all three chords, so the note box and the bench's composer agree on what a paste is.

## Where a pasted picture lives (decided: A)

`$XDG_STATE_HOME/terminal-delight/notes/pictures/<doc>-<hash>/doc-image-<n>.<ext>`, where `<doc>-<hash>` is the name `md_notes::store_for` already gives the document's notes file. One place for both kinds of document, outside every repository, and no brief grows. The picture does not travel with the file; a brief carried to another machine lists a picture that is not there, and the list says so rather than dropping it.

The folder also holds `images.json`, the document's list: `{format, file, next, images: {<anchor>: [{n, file, ts}]}}`. It is TD's own, for briefs and Markdown files alike, so neither the brief's notes island (a format shared with `notes.js` and pinned by fixtures) nor the Markdown store changes. `next` only ever counts up, so a deleted `doc-image #2` is never reused and a note that says "see doc-image #2" cannot come to mean a different picture.

## How it works

- **The chord.** While a note box has a caret, the three paste chords are the note box's, on every face: `pane_chord` stops claiming Ctrl+Shift+V when `Up` says a note is being written (`float_caret` on the square, a new `doc_caret` on the Document face). The pane's other chords still work over a note.
- **Reading the clipboard.** `DocumentView::key` reads it, because `NotesLayer` never touches the clipboard, and hands the layer a `Pasted`: words (text, and copied files as their paths, one word each), a picture, or nothing. gpui's Wayland read takes UTF-8 text first and falls back to an image, and never reads markup as text.
- **Words.** Line breaks kept (`\r\n` and a lone `\r` become `\n`), every other control character dropped but the tab, inserted over any selection, capped at `MAX_NOTE_CHARS`.
- **A picture (slice 2).** Written as `doc-image-<n>.<ext>`, with the format gpui reports; the element's list gains the entry at once, as a Markdown note is kept at once, so nothing waits on a save. Pasting the same bytes onto the same element twice adds nothing and says it is already there as `doc-image #n`. An image file copied in the file manager is attached the same way, copied into the folder; any other file pastes as its path.
- **The list in the box.** Under the notes, one row per picture: `[doc-image #n]` and when it was pasted, with ✕. A picture whose file is missing says *not on this machine* in its row. No thumbnail and no path.
- **The map.** Unchanged when no element has a picture, byte for byte, which the fixtures hold. With pictures, the header counts them (`· 2 pictures`), one legend line says what a `[doc-image #n]` line is, and each picture follows its element's notes as `· [doc-image #n] <full path>`. An element with pictures and no notes still gets its line, as a concur alone does.

## Slices

1. **Text paste (tracer).** The three chords paste words into a note on the square and on the Document face; Ctrl+Shift+V never reaches the terminal while a note is being written; a cut paste is said. Held by the key-ladder table (every combination of surfaces against the three chords), `NotesLayer` paste tests (caret, selection, line breaks, control characters, the cap), the clipboard sorting test, and two pane tests driving real keys: one on the square that finds the terminal's input empty afterwards, one on the Document face.
2. **Picture paste.** Kept as a file, attached to the element, listed, deletable, mapped. Held by unit tests for the folder and list (numbering, never reusing a number, dedupe, a missing file), the map with and without pictures (without: byte-identical to today), and a pane test pasting a picture from the test clipboard. Scripted runs paste from a file through a control verb, never the real clipboard, which is Parker's.
3. **The agent gets the picture.** `document_notes` answers the map and attaches each picture as an MCP image block, naming by path any picture too large to attach.

## Least-confident decisions

1. The list lives in TD's folder even for a brief, whose notes live in the brief. A brief's pictures cannot travel with it anyway, and keeping the list out of the island leaves the format `notes.js` shares untouched. The cost: a brief's notes and its pictures are kept in two places.
2. Pictures are numbered per document, not per element: `doc-image #3` names one picture in the whole brief, which is what makes it safe to mention in a note on a different element.
3. A picture wins nothing over text: gpui's read decides, and it takes text first. Omarchy's clipboard manager and every screenshot tool offer an image alone, so the common case is a picture.
4. ✕ on a picture deletes its entry and its file. A deleted picture's number is never given out again.

## Not in this plan

Drawing a picture anywhere in the document or the box; clicking a picture; dropping a picture onto the note box; editing a note after it is added; copy and cut inside the draft; pasting a picture into a brief opened in a browser (that is `notes.js`, upstream); a picture in the Markdown file itself.
