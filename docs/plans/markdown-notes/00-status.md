# Status: Notes on a Markdown document

**Difficulty: 6/10.** Markdown in TD is drawn natively by gpui rather than through Chromium, so it has no notes island, no `notes.js` and no anchors of its own. Every piece of the brief's notes layer has to be given something to stand on. The layer itself (note box, buttons, bar, copy map, ↪) is reused whole. What is new is the anchors, the storage and the map's wording. A wrong storage call is expensive to unwind, because notes already written would have to be migrated. That buys **one combined plan page and one approval.**
**Turned out to be:** 6, about right. One real decision (where the notes live), taken at the one approval; everything else reused the brief's layer as planned.

- Plan (product, architecture, slices in one): **APPROVED 2026-09-25**, choosing **A, TD's own store** ("TD's own store (Recommended)") over notes inside the file. Nothing else changed. `01-plan.md`.

## Slices
- [x] Slice 1: notes on a Markdown file's top-level blocks, kept in TD's store as written, copied and sent
- [x] Slice 2: list items as anchors of their own
- [x] Slice 3: a changed file — blocks re-identified, orphaned notes shown and mapped, never dropped. Also, not in the plan: the bar's "N on words no longer here" opens them to be deleted, because a note nobody can delete would ride along in every map sent.

## What was not done, on purpose
- Re-attaching a note to a block by its quoted words after the block's opening changed (least-confident decision 1). A guessed match can put a note on the wrong block, and the reader would then argue with a passage they never commented on. Orphans stay visible, mapped under the words they were written on, and deletable.
- A nested list item takes no note of its own; its note goes on the top-level item (least-confident decision 2), as planned.

## Verified
- `scripts/doc-md-notes-check.sh`, against a release build in a hidden window: a note kept at once with 0 unsaved, in the store and not in the `.md` (byte for byte), the map naming `[L7]` and `[L10] • two`, the agent's `document_notes` map equal to the bar's byte for byte, the note surviving a close and reopen, moving under "On words no longer in the file" when the heading changed on disk, and gone from map and store once deleted; the stand-in agent's stdin empty.
- `scripts/doc-send-check.sh` and `scripts/doc-notes-check.sh` (briefs) still pass on the same build.
- Not seen on screen: the gutter, the 💬 placement and the rule were checked only through their numbers, since photographing a window would take the screen.

## Context
- Asked 2026-09-25 alongside "↪ should also save" (PR #811, done separately): *"can we add commenting to MD files? I know we did that in markdown delight... would be nice to comment markdown files and SEND that map to an agent"*.
- markdown-delight's comment store (`~/.config/markdown-delight/comments/`) held one 19-byte file on 2026-09-25, so there is no body of existing comments to stay compatible with.
