# A projectless agent, and the T3 scuttle's surfaces made consistent in every look

**Difficulty: 3 / 10 for the projectless agent and 4 / 10 for the pass, scored late.** The
projectless agent was scored before its code (one function changes where a "none" agent
starts; wrong shows at once). The UX pass was not scored before it began, and is scored here
honestly after: a broad visual pass whose mistakes are cosmetic and cheap to undo, so one
page. Calls are in `polish.calls.json`; Parker was away and said not to ask.

Branch: `explore/t3-scuttle`.

## What Parker asked for (2026-11-09)

> Need to be able to laucnh a projectless thread / agent

> Then after you are all good - do a THROUGH pass on the UX and manual test everythign ( i
> will be afk so you have the computer) --- and make sure that all the skin and design
> combinations are SUPER nice and consistent with all the new builds we did on the T3
> scuttle

## The projectless agent

"none — don't know yet" (built in round two for *"I don't know the project yet"*) started
the agent in the home folder. It now starts it the way T3 Code starts a "No project" thread:
in a new folder of its own, `~/.scratch/<date>-<first five words>`, claimed with a
non-recursive mkdir; on a far machine the ssh line makes it first. Backspace on the PROJECT
chip, with nothing typed, selects none. Verified end to end on the private display with a
stand-in local runner (commit 477c4134).

## The pass

`rig/skin-sweep.sh` photographs ten surfaces (the relate dialog; the side panel's Changes,
File and + faces; the INBOX; the composer and its project list; the add-a-project dialog's
three steps) under eleven looks: the six built-in skins and Parker's own on his Omarchy
palette, then the four built-in designs (hacker, deco, clean, intrusive) on their own. Each
look gets a contact sheet. The first sweep is kept as the before set.

What it found, and what changed:

- **Secondary text vanished in the hacker and intrusive designs.** The composer, the
  add-a-project dialog, the relate dialog and the inbox drew hints, paths, counts and edges in
  the palette's raw `faint` (#14401f in hacker, on black). The side panel used the skin's inks
  and stayed readable. All four now use `ink_faint`, `ink_dim`, `edge_rest` and `rule`; a
  source test keeps the raw colour out of all five files.
- **Chalk's taller letters ran the project list over the waiting list below it**: the list had
  a fixed maximum height. It is capped by rows already, so the height cap went.
- **The inbox face's `+ NEW` ran under the collapse arrow** in chalk, and in Spanish, French
  and Russian in every skin. It is now the same `+` the projects face ends in, with a tooltip,
  and the two face buttons give way with an ellipsis instead of overlapping.
- **The project list hid the chosen project** when it was the ninth or later: it always opened
  at its top. It now keeps the choice in view and counts what is below.
- **`＋ add` beside PROJECT was a bordered button among borderless chips** in Parker's own
  skin. It is a chip now, with the skin's own word for a control (terminimalist spells it out).
- **Smaller:** the dialog's key hints are the composer's size; the folder step's hint lost its
  first item so it fits one line; a lit row's path is readable on the row's wash.
- **A regression of my own, caught by the second sweep:** with the inks fixed, hacker's lit
  chip (solid bright green) swallowed its PROJECT label. The label now dims the chip's own ink.

Then a tour of Parker's own look by hand, with the mouse (the sweep drives keys only), on the
private display. Photographs cannot show what moves, and the tour found four more:

- **Both centred cards jumped while a list narrowed.** Typing two letters into the composer's
  project list moved the sentence box 77 px down the screen; opening a short folder moved the
  add-a-project dialog's path field 58 px and every row with it, so the next click landed on a
  row nobody pointed at. Blank rows now hold the project list at eight rows and the folder list
  at ten, with the "more" and hint lines always present. Measured on the tour's frames: 0 px.
- **Back from the clone field, opened from the machines, landed on a folder list** the person
  had never seen. It now returns to the machines (`Browse::from_machines`).
- **The PROJECT chip's "none — don't know yet" wrapped REACH onto a second line.** The chip
  says `none` (`project_none_chip`, nine languages); the list row keeps the full wording.

A second tour, in the clean design, clicked the rest (relate swap and Draw the line, the side
panel's search, rows and tabs, every composer chip, the inbox's shelves and faces). Four more:

- **The composer's chips vanished at rest in the clean design.** `Skin::chip` leaves all but
  the glow and bar skins in `ink_off` (Faint), which that palette draws near its card. A new
  `Skin::control_chip` rests at `ink_dim`, as the tabs do (their test holds `ink_dim` to 3:1 on
  every palette); paper keeps its own control. Only the composer uses it.
- **Failures wore the accent,** which is green in hacker and clean and read as success: the
  relate dialog's refusal and the add-a-project dialog's failures (a taken name, a failed
  clone, ssh trouble, an unreadable folder). They wear `ink.danger`, as the side panel's
  failures do; the folder step's note carries a failure flag, and guidance keeps the accent.
- **A press on the side panel under the composer or the relate dialog lit its field** while
  every key went to the card. `card_holds_keys` now stops `flyout_focus` and `search_start`.
  Putting the card away instead was passed over: dropping the composer kills a clone in flight.
- **The PROJECTS face squeezed the inbox's face to "INBOX…",** dropping its count. The word
  gives way, the count never does, and both faces say their whole name on hover. Checked on
  the private display only: the inbox's waiting rows exist only in the demo.

## Progress

- [x] Projectless agent: model, composer, strings, tests, end to end on the private display
- [x] Sweep 1 (before): eleven looks, 110 shots
- [x] Fixes above, with tests (list window, inbox +, skin inks)
- [x] Sweep 2 (after) on the build with the ink fixes
- [x] Mouse tour of Parker's own look; four more fixes, with tests that fail without them
- [x] Second tour (clean design, then the inbox in Parker's own skin and chalk); four more fixes
- [x] Sweep 4 on the final build: 110 shots, none blank, every step reached in every look
- [x] Full suite in one run, no reruns: 3,529 passed, 0 failed, 18 ignored; clippy and fmt clean
- [x] Pushed (e3a549c9); brief written; the workspace 7 test window relaunched on the final build

## Outcome

Sixteen problems fixed: eight from the sweeps, eight from the two mouse tours, which is the
finding worth keeping: a still photograph cannot show a card that moves, a key that goes
elsewhere, or a field that lights for nothing. One regression of the pass's own (hacker's lit
chip swallowing its PROJECT label) was caught by the second sweep. Every fix but the inbox count
has a test shown to fail without it; the count was checked on the display, as the inbox's
waiting rows exist only in the demo.

**Post-hoc difficulty: 5 / 10** (scored 4 late). The pass reached two shared primitives, a skin
method and the side panel's keyboard routing, where a mistake reaches beyond the T3 surfaces; it
stayed cheap to undo, and every change is a two-way door. Eleven calls wait for Parker's stamp.

Brief: `~/Work/terminal-delight/reports/2026-11-09-t3-polish.html` (local, untracked).
