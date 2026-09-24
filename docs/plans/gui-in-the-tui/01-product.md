# Product: GUI in the TUI — documents open in the pane

## Problem
When an agent hands back something to read — a decision brief, a Markdown file, a screenshot — opening it throws Parker out of the terminal. The desktop opens a separate window, the tiling shrinks the terminal to make room, and the document now sits in a different window from the agent that wrote it and the conversation about it. In his words: *"reading documentation flips to another window."* He wants to read it where the work is: floating over the pane for a glance, or beside the agent for side-by-side reading and prompting. And a decision brief is only useful if he can comment on it and save those comments into the file, so that has to work inside the terminal too.

## Decisions (Parker, 2026-09-24)
- **Ctrl+click** still hands the file to the desktop — *"ctrl+click stays as expected functionality."*
- **Shift+click** moves to showing the file in the file manager — *"shift click feels like it should move to open file location."*
- **Alt+click** opens the document in a square floating over the pane — *"use alt+left click to hop into the overlay"* — in the glow TD's menus and overlays already have: *"use the hyperglow of our menus and overlays!"*
- **Clicking the floating square again docks it into a split** — *"WITH THE CLICK AGAIN TO SPLIT!!!"*
- **Ctrl+Alt+click** opens a side-by-side split straight away — *"ctrl+alt+left click to do a SPLIT right away… for side by side reading + prompting."*
- **Formats now:** Markdown, HTML and images. Video and other program-drawn pictures come later — *"for now… we stick to md img png etc. and html."*
- **A decision brief keeps its comments and its save** — *"our decision HTML which allows comments and SAVE doc… those actions MUST be available."*
- **Start simple and keep the HTML view replaceable** — *"Start with image yes… if we modularize and integrate correctly, it will be sane to upgrade to more interactive builds."*
- **Documents take the terminal's curved-glass look**, like everything else in the pane; revisit only if dense text reads badly.

## Success metric
_Proposed here, not in Parker's notes — strike or replace._ Over the first week after it ships, **more than half of the documents opened from a pane are read inside Terminal Delight** rather than handed to the desktop, counted by which click opened each one (Alt or Ctrl+Alt against Ctrl). A second number rides with it: how many briefs get a note saved from inside TD.

## Announcement — the blog post before the feature
_Draft, mine._ Terminal Delight now opens what your agents hand you right where you're working. Alt+click a Markdown file, an image or an HTML brief in a pane and it floats over the conversation in the same glow as TD's menus — drag it aside, press Esc to close it, or click it again to dock it beside the agent. Ctrl+Alt+click puts it in a split straight away, so you can read on one side and answer on the other. Decision briefs keep their notes: comment on any figure or question, save, and the notes go into the file itself, where the agent that wrote it can read them. Ctrl+click still hands a file to your desktop, and Shift+click now shows it in your file manager.

## Screens
- `mockups/01-alt-click-float.html` — holding Alt over a document link, then the floating square over an agent pane
- `mockups/02-ctrl-alt-click-split.html` — the same document in a side-by-side split
- `mockups/03-brief-with-notes.html` — an HTML decision brief open in TD, taking a note and saving it into the file
- `mockups/04-the-clicks.html` — every click on a path, before and after

## Not in this feature
Video and program-drawn pictures, editing documents, browsing the web, and live interactive pages — the HTML view starts as a picture of the page with TD's own notes on top.
