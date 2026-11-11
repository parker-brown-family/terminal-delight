# Every chip opens a list, and No project always leads

**Difficulty: 4 / 10.** One list, drawn for five more chips, in a centred card that has to
hold still and read right in eleven looks. Wrong shows at once on an explore branch and costs
a revert, so one plan page. Parker asked for the change in so many words, so it was built
without waiting on this page; the calls nobody made are in `pickers.calls.json`.

Branch: `explore/t3-scuttle`.

## What Parker asked for (2026-11-09)

Two screenshots side by side: the composer's PROJECT list, and T3 Code's project picker with
"No project" as its first, lit row.

> do we have a mega simple no project option?

Then the composer's chip row beside T3's model picker (a list with a search field, the
reasoning levels in a column, Ctrl+1–4 on the models):

> see how t3 does htis — we want to act a BIT more modern --- the arros to select options are
> a pain if someone has say 5 options or more - get that fix in

## What was there

- **No project existed, and hid.** "none — don't know yet" was row 0 of the PROJECT list,
  and the list keeps the chosen project in view. Parker's project was the ninth row, so the
  window started at row 1 and none was never drawn. Backspace on the empty chip chose it, but
  nothing said so.
- **Only PROJECT had a list.** HARNESS, MODEL, EFFORT, REACH and MACHINE were stepped blind
  with the arrows (or a click), one value at a time, and nothing showed what came next.

## What changes

1. **No project leads the project list, always.** It is drawn first whatever the window
   shows, in T3's words ("No project"), with its folder beside it and ⌫ at its end while
   nothing is typed. Typing hides it unless the words ask for it ("no project", "none", or
   its translation), the way T3's row has search terms.
2. **Every chip opens a list, in the same place.** The chosen option is lit; a click picks it
   and goes back to the sentence; typed letters narrow the list and take its first match;
   the arrows step through it as before, now in view. Beside each option, a quieter word
   where one helps: the model's wire id, the effort a model takes by itself, the flags a
   reach adds, a far machine's ssh destination, what `local` runs.
3. **One height for every list.** The project list is its add row, a status line, eight rows
   and a count; the others are a status line, nine rows and a count. The add row has a
   row's measure, so the card does not move while Tab walks the chips.
4. **Enter on a chip picks and goes back to the sentence** (T3's "Enter Select"). Enter in
   the sentence starts; Ctrl+Enter still starts in the background from anywhere.
5. **A click on the chip that has the keyboard closes its list** rather than stepping it.
6. **The key line names the list's keys** while a chip has the keyboard.

Not built: T3's Ctrl+1–9 on rows (typing a few letters reaches any row, and numbers that
move with a scrolling window mislead), a model and effort picked in one two-column popover,
a pointer wheel over the project list.

## What the photographs found

The sweep (`rig/pickers-sweep.sh`: nine frames per look, eleven looks) found two things the
tests had not, and each now has a test.

- **The card bobbed.** With a far machine listed, the chips fill a line; a long project name,
  or letters typed into a chip, wrapped the last chip onto a second line and a short one
  unwrapped it. The card is centred, so it moved 11 px each way under the pointer as the
  arrows passed long and short names (Parker's own skin: top edge 266 or 277). While a list is open,
  the chip row now shares a grid cell with two invisible chips, so it stands two chip lines
  tall whatever the values; the card's top edge sat at 266 in all eight list frames after.
- **The fix's own slip.** Stretched to the taller cell, the chips grew with it and a lit
  chip's bar ran two lines. The row now keeps to the top of the cell at its own height
  (`self_start`), and the test compares a chip's height with the list open and closed.

## Progress

- [x] Pure: `entry::Choice`, `entry::narrow`, `entry::asks_for_none`, with unit tests
- [x] Composer: chip lists, pinned No project, keys, clicks, footer
- [x] Words in nine languages (`composer_narrow`, `composer_list_hint`, `project_none`)
- [x] gpui tests: none always drawn; a list per chip; the card holds still across chips and
      across a wrap; typed letters pick; Enter goes back to the sentence; a click picks
- [x] Mutation checks: nine, one per guarantee, each failed its test
- [x] Sweep of the eleven looks with the lists open (three times: before, the slip, after)
- [x] Brief, calls, commit, push

## Calls

Parker settled three in chat on 2026-11-09, each the built option: the lists open under the
chips (not a popover at each chip, with MODEL and EFFORT side by side), Enter on a chip picks
and goes back, and T3's Ctrl+1–9 stay out. Three are open in `pickers.calls.json`, for him to
stamp: no project pinned and found by its words, the lit row as the chip's value while the
arrows move, and the word beside each option.

## Outcome

**Post-hoc: 4 / 10 held.** The change was what it looked like, one list for five more chips,
and the two problems it met were both visual and both caught by the photographs within the
round. The least-confident call is the live choice: the arrows change the value as they
move, so Esc keeps what was arrowed to rather than cancelling, which is how the chips already
behaved.
