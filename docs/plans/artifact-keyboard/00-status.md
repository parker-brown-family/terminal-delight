# Reading an artifact from the keyboard — status

**Difficulty: 6/10.** This is one surface, but it reverses the floating square's "read beside the shell" key rule. It also changes which box in a stack hears the keyboard. A routing mistake would show up only when a video sits over a page. That bought one combined plan page and one approval.

- Plan page: `brief.html` (source `brief.src.html`). It carries Parker's notes and stamps as he left them on 2026-11-02.
- Gate: **approved 2026-11-02.** Questions 1–4 are stamped CONCUR. On question 5 (slice order) Parker wrote *"Go all out I am afk"*, and in chat *"GO! end to end"*.
- Built on branch `artifact-reading`, on top of the sibling plan's slice 1 (PR 1285: Alt+O, Alt+Q, Alt+E, Alt+0, pane marks).

## What Parker added on the page

- *"agents will AUTO-OPEN artifacts, and THOSE should be small, but if I alt+o … that should open the READER version"*. A person's open (`open_to_read`) goes to the frame; an agent's `open_document` keeps the square.
- On the frame (question 2): *"should we simplify by borrowing the reader or no? I leave this 100% up to your good judgement"*. It borrows the reader. FOCUS already drew a pane's document at 80% of the window and passed it the pointer, so the reading frame is FOCUS with Parker's geometry.
- On the send key (question 4): *"AND CLOSES THE ARTIFACT! assume that send means done"*. The document and frame close once the workspace reports the map landed (`Said::Done`). A refused send leaves the document open.
- On the ladder figure: *"This would make a cool bit of the Terminal Delight video! WHERE EXACTLY does your keystroke land when..."* This is not done. It is for whoever plans the next film.

## What was built

- `keylayer::Layer::Reading` sits between Rename and Float. `Up::reading` is set while a page or Markdown file is lent to the reader. `read_key` maps keys to `ReadKey`. The pane sets `reader: false` while reading, so Escape and paging come through the one ladder. Ctrl+F and Ctrl+X are no longer pane chords while reading, because they reach the shell.
- `ReaderGlass::in_window`: 80% wide, 5% above, 10% below (85% tall), for the terminal reader too.
- `TerminalView::open_to_read` is called from Alt+click, the right-click menu, bench links, the artifact chip and rail (so Alt+O), and links pressed inside the reader. `FloatingDoc::to_read` means the document closes with the frame.
- A clip over a page being read: the reader lends the page (`read_document`) and draws the clip on top of it (`reader_clip`, `focus_clip_box`). Presses inside the clip go to the clip. Keys go to the page.
- Keyboard targets: `NotesLayer::select`, `notes_ui::pick` and `into_view`, `Mark::Selected` (the ring), `ReadAct`, and `Backend::read_act` for pages and Markdown. The notes report gains `selected`.
- The header hint `focus_read_keys`, in all nine languages.

## Calls made without Parker (overturn any)

- One reading frame per window: FOCUS reads one pane at a time.
- Images and PDFs keep today's square and keys.
- Find-in-page (`/`) is left out. It is unknown whether the page picture carries word positions.
- Alt+R on an agent's small square also reads the document with the reading keys, because the rule is "a page or Markdown file in the frame is read". Escape then closes only the reader, and the square stays.
- The F1 help sheet does not list the reading keys yet. The frame's own header does.

## The glow pane reads too (2026-11-05)

Parker, the day the FLOATING DOCUMENT toggle landed: *"keyboard only --- needs to beahave just like reader mode! scrolling. enter to add comment, etc. + ctrl + shift + enter"*, with the box *"centered at 5% top and bottom padded and 10% left right padded againts the active pane"*. His question was whether the box swallows what would reach the terminal: *"Simple answer is yes, we imagine the doc glowbox artifact is PRIMARY attention.. however we do not want to eat the other keystrokes for general TDS funcaitonlity."*

- `FloatingDoc::in_glow` puts a person's open in the glow pane on the same `Layer::Reading` rung as the frame, through `reading_view`. There is no new rung or `Up` bit, so the two surfaces cannot drift apart.
- `docopen::glow_home` is the box's place, measured against the pane's screen (below its header). `FloatingDoc::seated` keeps it there as the pane changes size, until a person moves or resizes it.
- Two leaks in reading itself were closed for both surfaces. Ctrl+PgUp/PgDn and their shifted pair were eaten, and now leave for the window (`tab_page_chord`). Ctrl+Shift+V and Shift+Insert pasted into the shell behind the document, and now stop there unless a note box takes them (`keeps_paste`).

Calls made without Parker (overturn any):

- An agent's own square is not read: it stays small and leaves typing with the shell, because an auto-open must not take a prompt being typed (*"agents will AUTO-OPEN artifacts, and THOSE should be small"*). Alt+R reads it in the frame, and choosing it from the Alt+O list reopens it to read.
- Switching GAUGES to glow pane moves only a document being read. A small agent square stays what it is.
- Ctrl+F and Ctrl+X stay swallowed as they are in the frame, and so does Ctrl+C. Ctrl+Shift+F still finds across panes.
- The padding is measured from the pane's screen, below its header, because that is the space a square is drawn in.
- The keys are shown along the box's foot, reusing the frame's translated line, because the strip across the top is full.

## Evidence

- `keylayer.rs` standalone: 43 tests, including four new reading-rung tests. The property test now covers all 65,536 layer states.
- Pane behaviour tests: `a_brief_opened_by_a_person_is_read_from_the_keyboard`, `a_clip_over_a_page_being_read_leaves_the_page_the_keys`, and `an_agent_opening_a_brief_keeps_the_small_square`. The first two fail when the Reading rung is disabled, which was checked by mutation.

## Post-hoc difficulty

6/10 held. Most of the cost was reading how FOCUS lends views, not writing code. The approval gate was worth it: three of Parker's four notes changed what got built.
