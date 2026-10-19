# Notes on a Markdown document: plan

## What a person gets

Alt-click a `.md` file and it opens in the floating square as it does today. Now each block also takes a note: a heading, a paragraph, one item of a list, a code block, a table or a quote. A 💬 shows at the block's top right while the pointer is over it, and the block keeps a count and a rule down its left edge once it holds notes. That is the brief's own look. A press opens the same note box a brief uses (Ctrl+Enter adds). The same bar sits bottom-right: **N notes · ⎘ copy map · ↪ send to agent**.

The map is what reaches the agent, pasted into its prompt as ↪ pastes a brief's today, or read by the agent itself through `document_notes`. It names lines, not element ids, because an agent opens a Markdown file at a line:

```
NOTES — /home/parker/Work/terminal-delight/docs/plans/x/01-plan.md
3 notes on 2 blocks.
Each [L<n>] is the line in that file where the block starts.

[L12] ## Slice 2 — the agent side
  · this should come before slice 1
[L40] - Store under XDG state, keyed by the file's canonical path
  · why not beside the file?
  · and what happens on a rename
```

## The decision: where the notes live

A brief keeps its notes inside itself, in an island, and "save into file" writes them there. A Markdown file has no such place, and the files TD opens are mostly tracked in git: plans, handoffs, READMEs, `AGENTS.md`.

- **A. TD's own store, off the file (recommended).** `$XDG_STATE_HOME/terminal-delight/notes/<hash of canonical path>.json`, written on every edit, so there is nothing to save and the bar has no 💾. No repo is dirtied, no note is swept into another agent's commit, and no note lands in an `AGENTS.md` that every agent loads into context. This is the reasoning markdown-delight took. The cost: the notes do not travel with the file, and a rename leaves them behind. They are then shown as notes on a file no longer there, never dropped.
- **B. Inside the file, as a hidden block at the end** (`<!-- terminal-delight notes … -->`), saved by 💾 and by ↪ as a brief's are. The notes travel with the file and an agent reading it sees them. The cost is everything A avoids: every note is a diff in a tracked file.

## How it works

- **Anchors.** The renderer already records each top-level block's y at paint. It will record the whole box, plus each top-level list item's. A block's id is the slug of its first words, as `notes.js` makes one, with `-2`, `-3` for repeats. Its title is its text cut at 72 characters, and its line is comrak's source line.
- **The layer.** `NotesLayer` is used as it is. The Markdown backend hands it the blocks as anchors, and a view-to-view mapping stands in for the page's CSS-to-view one. `build_map` gains the line form of the legend and anchor shown above. The brief's form is unchanged, byte for byte, and held by its fixtures.
- **The backend.** `MarkdownDoc` implements the notes methods of `Backend` that only the page implements today: press, hover, key, caret, beside, said, report, command. A press goes to the layer first, then to the marks, then to links. `guard_close` answers false under A, where nothing can be lost.
- **When the file changes.** Ids are worked out again. A note whose block is gone stays in the count and in the map, under **"on text no longer in the file"** with the words it was written against. The type keeps "block not found" apart from "no notes".
- **Agents.** `document_notes` and `ctl doc notes` read `notes_report()`, so a Markdown file answers them without a change to either.

## Slices

1. **Tracer.** Notes on a Markdown file's top-level blocks: add, delete, count, copy map, ↪ (A: kept as written). Held by unit tests, and by a hidden-window script in the shape of `doc-send-check.sh` that adds a note over `ctl doc note add` and reads the map back through the agent's `document_notes`.
2. **List items** as anchors of their own.
3. **A changed file.** Blocks re-identified, orphaned notes shown and mapped, never dropped.

## Least-confident decisions

1. Ids from the first words, as `notes.js` does, mean that editing a block's opening orphans its notes. The fallback, matching on the quoted text, is in slice 3.
2. Top-level list items only in slice 2. A nested item's note goes on its parent item.
