# Gate 1 — Product: Terminal Delight on Rio's core

**State:** self-approved 2026-09-25 under Parker's instruction to build the swap
while he was away. Every line below is his to overrule.

## What changes for the person at the keyboard

Terminal Delight stops dropping the pictures programs draw. Today a program
that speaks the Kitty graphics protocol — `kitten icat`, chafa, a ratatui image
widget, an agent showing a chart — sends a picture into a pane and the emulator
throws it away, and the text after it moves up into the space the picture
should have taken. After the swap the picture appears where the program put it.

The pictures are **attentional**, in Parker's word. A picture is there while you
are looking at its pane. Switch to another tab and it goes; it is not kept in a
snapshot, replayed on reattach, or carried in scrollback for later. That is a
product decision, not a limitation to apologise for: TD's pictures are for the
moment of attention, like the floating square that Alt+click opens today.

Everything else should feel the same, only faster and lighter. Measured on the
research branch on this machine: an 8 MB text stream parses in 82 ms on rio-vt
against 140 ms on alacritty_terminal, and a pane holding 10,000 lines of history
costs 11.0 MB against 28.5 MB.

A small bug goes away as a side effect: `kitten icat` currently waits ten seconds
in every TD pane because TD's device-attributes answer (`ESC[?6c`) is too short
for its detector (issue 750). rio-vt answers `ESC[?62;4;6;22;52c`.

## What must not regress

The swap is judged first on what it must not break. Each item names how it is
checked.

| Must keep | Checked by |
|---|---|
| Text, colours (16, 256, truecolour), bold, dim, italic, inverse, hidden, strikeout, five underline styles and underline colour | the correctness matrix and the snapshot round trip, run against the new core |
| Wide characters, emoji, combining marks, soft-wrapped lines | the same, plus the wide-character and wrap regressions the round trip already names |
| Scrollback, the FOCUS reader reading history, find, jump between your turns | the reader's history test and the find tests |
| Selection by character, word and line, shift-extend, copy, PRIMARY | selection tests, then by hand in a new window |
| Mouse reporting, bracketed paste, application cursor and keypad, focus reporting, alternate screen | mode tests, then vim, htop and less by hand |
| Replies to programs' questions answered exactly once, by whoever owns the PTY | the replica tests that already assert this |
| The session host and its windows: attach, snapshot, live stream, divergence guard, reattach | the host/replica suite and `grid_hash` agreement |
| Keystroke latency | `scripts/td-echo-bench.sh`, before and after |
| Twenty panes | the twenty-pane memory and speed run from the research |
| Session restore, demo mode, pane mode detection, MCP pane text | their existing tests, then a demo window |

## Out of scope for this change

- **Sixel and iTerm2 pictures.** rio-vt decodes them; drawing them is a later
  change once Kitty pictures have proved the drawing path.
- **Persistent pictures.** Snapshots, reattach and scrollback carry no pictures,
  by the attentional rule above.
- **Changing `TERM`.** Panes keep the `TERM` they have today. What a terminal
  calls itself is a separate decision with its own blast radius.
- **Rio's renderer, windowing or fonts.** Only the emulator core moves; gpui and
  TD's drawing stay.

## Done means

1. rio-vt is the only terminal emulator core in the release build.
2. The whole test suite is green, including the correctness matrix and the
   snapshot round trip, now written against TD's own core boundary.
3. The CI gates are green: `cargo fmt -- --check`, `cargo check --locked`,
   `cargo clippy --locked -- -D warnings`, `cargo test --locked`.
4. In a new TD window on its own session: a shell, vim, htop and `kitten icat`
   behave, a window attaches to a host-owned pane and matches it, and a picture
   appears and then vanishes when its tab is switched away.
5. Latency and twenty-pane numbers are no worse than alacritty's.
6. The docs site, README, glossary and licences name the new core.
