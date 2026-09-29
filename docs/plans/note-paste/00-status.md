# Status: Pasting text and pictures into a note

**Difficulty: 6/10.** Pasting text is small: the note box ignores Ctrl+V, and the pane takes Ctrl+Shift+V before the note box sees it. Pasting a picture touches more: TD's notes store, the note box's drawing, and the map an agent is handed. The expensive call is where a pasted picture is kept. Changing it later means moving every picture already pasted. That buys **one combined plan page and one approval**, the same as notes on a Markdown document.
**Turned out to be:** 6, about right. The expensive call was taken at the one approval, and Parker's answer to the second question (attach a picture to the element, list it, mask its path) made the build simpler than the brief's proposal: no format a browser shares had to change. All three slices landed the same day. What cost time was on the test side: a brief's Alt outline only appears once the pointer is over the page, the document view keeps a hand-written list of files its guards scan, and every MCP answer carries a trailing block naming the window that answered.

- Plan (product, architecture, slices in one): **APPROVED 2026-09-29**, through notes on `reports/2026-09-29-pasting-into-notes.html` (6 notes, 2 concurs). `01-plan.md` is the plan as approved; the brief keeps the proposal it amended.

## Decisions
1. **Where a pasted picture lives: A, TD's notes folder.** Concurred.
2. **How a note points at it: neither option.** Parker overrode both: a picture is **attached to the element**, not written into a note. The note box lists an element's pictures in a section under its notes as `[doc-image #1]`, `[doc-image #2]`, numbered per document, and never shows the path. The map lists each one under the element with its full path. Nothing is drawn in the document and nothing is clickable.
3. **Build slice 1 now.** Concurred.

## Slices
- [x] Slice 1: text pastes into a note on a brief and on a Markdown file, by Ctrl+V, Ctrl+Shift+V or Shift+Insert, and Ctrl+Shift+V never reaches the terminal while a note is being written. Merged in #894 as `4df81ee`: the whole suite green on the merged tree (2,054 passed), two pane tests pressing the real keys, one on each seat, and three mutations each caught.
- [x] Slice 2: a pasted picture attaches to the element whose note box is open, kept as a file in TD's notes folder, listed as `[doc-image #n]` with a delete, and mapped with its full path. Merged in #895 as `94554bf`, with pane tests pasting a real PNG into a note box on a Markdown file and on a brief (whose file stays byte for byte the same), and five mutations each caught.
- [x] Slice 3: `document_notes` hands an agent the pictures themselves: each after the map as an image block, at most 8 and each at most 3.75 MB, PNG, JPEG, GIF or WebP, read only if it is a regular file; any other is named by its path with why. A picture list naming a file outside its folder is refused whole, so nothing that reads pictures is sent elsewhere on disk. Merged in #896 as `bffd5cd`, four mutations each caught.

Installed as `td-bffd5cd-note-agent-pictures`, built from main with all three.

## Not verified
- **The picture list has not been looked at.** Opening a note box needs a pointer, and nothing here presses one in a hidden window, so its look in the box rests on the report of what was drawn. Parker is testing it live.
- **The caps on pictures handed to an agent** (eight, 3.75 MB each) are estimates of what a model and the MCP client take, not measured against either.

## Found on the way, filed
- Omarchy's Super+C sends Ctrl+C to a Terminal Delight terminal pane, which interrupts whatever runs there, and its Super+V and clipboard manager do not paste into one: TD's window is not tagged as a terminal for Omarchy's universal clipboard, and the terminal face does not answer Shift+Insert (#892). Read from Omarchy's config and TD's key encoding, not reproduced in a window.
- `a_connection_that_has_gone_stops_being_pushed_to` failed once more under a loaded full suite and passed on the re-run; noted on its open issue (#732).

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
