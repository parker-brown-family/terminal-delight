# Status: Pasting text and pictures into a note

**Difficulty: 6/10.** Pasting text is small: the note box ignores Ctrl+V, and the pane takes Ctrl+Shift+V before the note box sees it. Pasting a picture touches more: TD's notes store, the note box's drawing, and the map an agent is handed. The expensive call is where a pasted picture is kept. Changing it later means moving every picture already pasted. That buys **one combined plan page and one approval**, the same as notes on a Markdown document.

- Plan (product, architecture, slices in one): **APPROVED 2026-09-29**, through notes on `reports/2026-09-29-pasting-into-notes.html` (6 notes, 2 concurs). `01-plan.md` is the plan as approved; the brief keeps the proposal it amended.

## Decisions
1. **Where a pasted picture lives: A, TD's notes folder.** Concurred.
2. **How a note points at it: neither option.** Parker overrode both: a picture is **attached to the element**, not written into a note. The note box lists an element's pictures in a section under its notes as `[doc-image #1]`, `[doc-image #2]`, numbered per document, and never shows the path. The map lists each one under the element with its full path. Nothing is drawn in the document and nothing is clickable.
3. **Build slice 1 now.** Concurred.

## Slices
- [x] Slice 1: text pastes into a note on a brief and on a Markdown file, by Ctrl+V, Ctrl+Shift+V or Shift+Insert, and Ctrl+Shift+V never reaches the terminal while a note is being written. Merged in #894 as `4df81ee` and installed as `td-4df81ee-note-paste`: the whole suite green on the merged tree (2,054 passed), two pane tests pressing the real keys, one on each seat, and three mutations each caught. The terminal face's own trouble with Omarchy's chords is filed separately: "Omarchy's Super+C interrupts a Terminal Delight terminal pane, and Super+V and its clipboard manager do not paste into one" (#892).
- [x] Slice 2: a pasted picture attaches to the element whose note box is open, kept as a file in TD's notes folder, listed as `[doc-image #n]` with a delete, and mapped with its full path. Pull request #895, with pane tests pasting a real PNG into a note box on a Markdown file and on a brief (whose file stays byte for byte the same), and five mutations each caught. Not photographed: opening a note box needs a pointer, and no tool here presses one in a hidden window, so the list's look in the box is held only by what the report says was drawn.
- [x] Slice 3: `document_notes` hands an agent the pictures themselves: each after the map as an image block, at most 8 and each at most 3.75 MB, PNG, JPEG, GIF or WebP, read only if it is a regular file; any other is named by its path with why. A picture list naming a file outside its folder is now refused whole, so nothing that reads pictures is sent elsewhere on disk. Built on `feat/note-paste-agent-pictures`.

## Context
- Asked 2026-09-29: *"copy paste into comments for html md --- I would love to be able to paste in text and image into our comments...That sound tricky though... WHERE does the image live... dunno - let's figure it out!"*
- Parker on the brief, the design as approved:
  - *"I really like the convention that Claude uses, image number 6, image number 2. Maybe we can say doc dash image number 1, 2, 3, so that the incrementer doesn't get confused with is this living in the terminal, is this living somewhere else, but it's local to this doc. And not showing the user the full file path is going to be really key here, so we need to mask that as well."*
  - *"we will not render these images in the document. This is basically for a one-way pass of a screenshot gets taken and pasted into the comment box. We do not want to then be able to click on it. That is overbuilding for sure."*
  - *"Pasting an image into a comment shouldn't be in line. It will attach that image to the element … It gets added as a list item underneath the comments, and then we can put the full file name in there."*
  - *"in a section inside of the comments box, show the list of images that are attached to that element, and if the person is making a comment, they'll refer to that image."*
  - *"Consider that Omarchy has Super V, and then Omarchy has, I believe it's Super Control V … we should definitely build for Omarchy first."*
- Omarchy, read from `/usr/share/omarchy/default/hypr/bindings/clipboard.lua` and the clipboard manager's scripts: Super+V sends **Ctrl+V** to a window Omarchy does not tag as a terminal, and **Shift+Insert** to one it does. TD's `terminal-delight` app id is not in Omarchy's terminal pattern, so TD gets Ctrl+V. Super+Ctrl+V opens the clipboard manager, which puts the chosen entry (text or image) on the clipboard with `wl-copy` and then types **Shift+Insert** with `wtype`. The note box takes all three.
- Found while reading for the plan: Ctrl+V in a note box did nothing (`EditBuffer::apply` drops Ctrl chords), and Ctrl+Shift+V in a note box on the square was claimed by the pane's own chords and pasted into the terminal under the square (`paste_clipboard`). On the Document face the same chord did nothing.
- Adjacent, open: "a note draft taller than the note box cuts off Add note" (#878). A picture list makes the box taller, so slice 2 meets it.
