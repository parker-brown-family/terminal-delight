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
any off turns reporting off) and so does rio-vt; alacritty kept three
independent bits. The snapshot encoder wrote them one at a time, so a later
`?1003l` undid an earlier `?1002h` and a pane reporting drags arrived in a
replica reporting nothing. The encoder now restores them as one setting — all
off, then each that is on, weakest first — which lands correctly on either core.
`gridwire.rs`, `MOUSE_PROTOCOLS`. The hash is unchanged.

*Left as it is:* an old session host (alacritty) and a new window (rio-vt)
disagree about a program that turns on two mouse protocols, because alacritty
keeps both and rio-vt keeps the last. The divergence guard sees the mode bits
differ, re-snapshots once, and then leaves the pane alone — its standing rule.
The screen itself is identical. This lasts only while an old host is running.

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
  the programs that lay out their screens by `wcwidth`; a program can still turn
  it on.
- rio-vt emits render, damage and graphics-queue events TD does not read. The
  adapter drops them so they cannot move a pane's content generation.

## Cost of reading through the boundary

`reading_a_full_history_costs` (ignored instrument, release build, this
machine): the divergence hash over a 10,030-line terminal takes 29.6 ms on
rio-vt and 26.5 ms on alacritty through the same boundary; the snapshot an
attach sends takes 25.7 ms and 23.4 ms. Best of five runs each. rio-vt assembles
each cell from a packed word and a style table where alacritty copies a struct.
Both are run once per guard probe or attach, never per frame.
