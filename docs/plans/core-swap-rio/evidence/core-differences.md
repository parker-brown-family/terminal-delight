# Evidence: where rio-vt and alacritty part, and what TD did about each

Recorded 2026-09-25 while the suite moved from alacritty_terminal 0.26 to
rio-vt 0.5.28. The suite runs on both cores (`cargo test` for rio-vt,
`cargo test --features core-alacritty` for the fallback); each difference below
was found by a test failing on one core and passing on the other, or by reading
the source for a behaviour the suite did not reach, and says which.

On the first run on rio-vt, 1,873 of 1,874 unit tests and every integration
suite passed unchanged.

## Differences that needed a change

**The four mouse-reporting modes are one setting.** Found by
`gridwire::roundtrip::the_cursor_and_the_modes_come_back`. xterm treats DEC
9/1000/1002/1003 as one setting (turning one on turns the others off, turning
any off turns reporting off) and so does rio-vt. alacritty 0.26 does the first
half — setting one clears the others — but turning one off clears only that
one. The snapshot encoder wrote them one at a time, so a later `?1003l` undid an
earlier `?1002h` and a pane reporting drags arrived in a replica reporting
nothing. The encoder now restores them as one setting — all off, then each that
is on, weakest first — which lands correctly on either core. `gridwire.rs`,
`MOUSE_PROTOCOLS`. The hash is unchanged.

*Left as it is:* because both cores clear the others when a protocol is set, an
old session host (alacritty) and a new window (rio-vt) agree about every
program that turns protocols on, however many it names (corrected in review;
this note first said alacritty kept all three bits). They part only when a
program turns off a protocol other than the one that is on — `?1002h` then
`?1000l` leaves alacritty reporting drags and rio-vt reporting nothing. The
divergence guard sees the mode bits differ, re-snapshots once, and then leaves
the pane alone, its standing rule. This lasts only while an old host is
running.

**A blank is its background alone.** Found in review by hashing the same 94
screens with the build before the swap, which is what a running session host
still is, and with this one on both cores. alacritty through the boundary
matched the old build on all 94; rio-vt parted on every screen that erased,
scrolled, inserted or cleared while a pen was set. rio-vt keeps blanks two
ways, and neither reads back as alacritty or xterm writes them. An erase
(`K`, `X`, `@`, `P`) keeps the background inline and loses whether it was a
named colour, so `44` then `K` read as `48;5;4`. A scroll, an inserted or
deleted line and a cleared screen fill with the whole pen, so a line that
scrolled in while inverse or underline was on drew inverted or underlined
from edge to edge — visible, not only hashed. `blank_background` in `vt/rio.rs`
reads both as alacritty does: the background, a named colour where it can be
one, and nothing else. `gridwire::roundtrip::AS_THE_OLD_HOST_HASHED` pins all
92 screens that agree to the old build's hash, on both cores; the two that
part on purpose — a Kitty picture, which the old host drops, and a palette
index under 16 set with `48;5;n` and then erased, which now reads as named —
are pinned in `WHERE_RIO_PARTS`.

**rio-vt introduces itself as Rio.** Read from the source
(`crosswords/mod.rs`, `identify_terminal`, `report_version`,
`report_keyboard_mode`); the suite did not ask. The rio adapter now answers as
Terminal Delight, each pinned by a test in `vt/rio.rs`:

| Question | alacritty said | rio-vt says | TD now says | Why |
|---|---|---|---|---|
| Primary device attributes (`CSI c`) | `?6c` | `?62;4;6;22;52c` | `?62;22c` | rio's claims sixel (4) and OSC 52 (52), neither of which TD draws or acts on. TD's is still long enough for `kitten icat`'s detector, which is the fix for its ten-second wait (issue 750) |
| XTVERSION (`CSI > 0 q`) | nothing | `Rio 0.5.28` | `terminal-delight 0.3.0` | a program reading "Rio" would pick the picture protocol Rio prefers |
| Kitty keyboard query (`CSI ? u`) | nothing (protocol off in its config) | `?0u` | nothing | TD encodes keys itself, legacy only. Answering tells a program to expect encodings TD never sends |
| Kitty keyboard push (`CSI > n u`) | ignored | kept | kept by rio-vt, masked out of `TermMode` | same reason |

**`CSI 16 t` is answered by the core.** alacritty's parser dropped it, so only the
session host answered it, from a scanner in its tee. rio-vt answers it from the
cell size it was given, which TD now always gives it. The alacritty adapter
answers it the same way with the old scanner, so both cores agree and the host's
tee no longer scans. A window-owned pane now answers it too; before, it did
not answer at all.

## Differences that went the other way

**The generated-content round trip passes on rio-vt.**
`encode_then_replay_is_the_identity_over_generated_content` had been ignored
because it fails on a real encoder defect (a bold attribute reaching a trailing
blank, 11×4 seed 107). On rio-vt all 1,200 seeds pass, so it runs in the default
suite again and stays ignored only on the fallback. Why rio-vt never produces
that grid has not been established.

**A wide character is never torn in half.**
`a_wide_char_whose_spacer_was_erased_cannot_be_reprinted` fails its own premise
on rio-vt: its erase never leaves half a wide character, so the grid the test is
about cannot occur. It stays ignored, for the fallback.

## Differences absorbed in the adapter, with no test change

- An erased cell holds NUL in rio-vt and a space in alacritty. The adapter reads
  NUL as a space, which is what word selection, trimming and the snapshot
  expect.
- A history row in rio-vt keeps the width it was written at when the terminal
  grows wider. `Term` pads every row to the terminal's width.
- rio-vt checks its row indices only in debug builds. `Term` clamps every read.
- rio-vt defaults grapheme clustering (DEC 2027) on, which sizes cells by
  cluster rather than by `wcwidth`. It is switched off, matching alacritty and
  the programs that lay out their screens by `wcwidth`. Since review a program
  cannot turn it back on either: `vt/text.rs` drops the request, and DECRQM
  2027 is answered "not recognised", as alacritty answered, because the cluster
  path copies a cell's marks the way the next section describes.
- rio-vt emits render, damage and graphics-queue events TD does not read. The
  adapter drops them so they cannot move a pane's content generation.

## Found in review, and stopped at the boundary

Three second reviewers read the rio adapter against rio-vt's source, the
pictures, and the read loop, each with probes that fed both cores the same bytes
(2026-09-25). Four things rio-vt does are unsafe in a terminal whose output TD
does not control, and are stopped before its parser — `Core::advance` in
`vt/rio.rs` hands every read through `vt/kitty.rs`, `vt/text.rs` and
`vt/compat.rs` first:

- **Combining marks cost quadratic time.** rio-vt copies a cell's whole list of
  marks for each mark it adds. `e`, U+0301 and `ESC[65535b` (thirteen bytes)
  took 9.7 s and 2.8 GB; alacritty took 1.4 ms. A mark now reaches the core
  only straight after its character, 32 at most, and a repeat of a mark goes
  nowhere (`vt/text.rs`).
- **A picture command with no end is buffered without limit** (837 MB held for
  768 MB sent). One command is cut at 4 MiB (`vt/kitty.rs`).
- **A temporary picture file is deleted by a substring test**, so a path through
  a marked directory with `..` reached any file. The core is handed `t=f` and TD
  deletes by kitty's rule, after the core has parsed the command.
- **Shared memory is opened blocking**, so a FIFO in `/dev/shm` held the parser,
  and the terminal's lock, until written to. Only a regular file reaches it.

And three it did differently from alacritty in a way a person would see:

- **A synchronized update tore** when the read that opened it carried part of
  the frame: rio-vt holds back only later reads. `vt/text.rs` hands the core the
  rest of such a read as a read of its own.
- **Hiding a tab forgot only the screen on show.** A picture drawn before vim
  opened came back when vim quit, and the bytes of forgotten pictures stayed
  counted against rio-vt's 320 MB budget, so later pictures evicted visible ones.
  `forget_pictures` forgets both screens and gives the bytes back.
- **XTGETTCAP answered as Rio**, 80 by 24 whatever the pane, with sixel and
  iTerm2 pictures TD does not draw. It goes unanswered, as under alacritty.

## Left as they are, knowingly

Each of these makes a pane on a host from before the swap disagree with its
window for a while, and costs at most one repair by the divergence guard. None
is visible except where said:

- **Origin mode** (issue 839). Setting DECOM homes the cursor in alacritty and
  not in rio-vt. Making rio-vt home would misplace the cursor of every snapshot
  that restores origin mode, because the snapshot writes the mode after the
  cursor.
- **Width tables.** About 3,950 codepoints are sized differently, among them
  U+00AD, Hangul Jamo Extended-B and some unassigned plane-14 codepoints. rio-vt
  also drops VS15 and VS16 after a base that is not an emoji. Where such text is
  on screen, the difference lasts until it scrolls away.
- **Tabs over written spaces.** alacritty writes `\t` into a cell holding a
  space, and rio-vt only into an empty one.
- **`2J` in the primary screen** pushes a different number of lines into
  history, because rio-vt counts a written space as content.
- **Copying** (issue 839). A selection that ends at the end of the last line
  loses its trailing newline in rio-vt, which also trims a trailing space inside
  the selected range.
- **Reflow** of a wide character at a wrap boundary differs on resize.
- **Secondary device attributes** answer rio-vt's version, `528`.
- **Past 65,535 distinct styles alive in one terminal**, rio-vt draws new
  colours in the default. It lasts until the history holding them scrolls out
  (issue 836, with the other things to report upstream).
- **Sixel and iTerm2 pictures** are decoded and reserve their rows, but TD draws
  only Kitty pictures yet, so they leave a gap (issue 829).

## Cost of reading through the boundary

`reading_a_full_history_costs` (ignored instrument, release build, this
machine): the divergence hash over a 10,030-line terminal takes 29.6 ms on
rio-vt and 26.5 ms on alacritty through the same boundary; the snapshot an
attach sends takes 25.7 ms and 23.4 ms. Best of five runs each. rio-vt assembles
each cell from a packed word and a style table where alacritty copies a struct.
Both are run once per guard probe or attach, never per frame.
