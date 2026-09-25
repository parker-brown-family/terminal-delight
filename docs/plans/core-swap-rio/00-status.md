# Status: Core Swap — Terminal Delight moves to rio-vt

**Difficulty: 9/10** — the emulator core is under every pane, the session host
and every window, and the snapshot a window replays is built from its exact
grid semantics. A wrong swap does not crash; it draws a pane that looks right
and is wrong, and the host-to-window guard was built precisely because that
failure is silent. Unwinding it after other work lands on top is expensive.

**Built on 2026-09-25, unattended.** Parker made the decision ("swap out the
Terminal Delight terminal core to use Rio") and went AFK for the day with the
instruction to architect it well and test it in new TD windows. There is no
approval channel, so each gate below is written in full, reviewed
adversarially, and marked **self-approved under that instruction**. Every
choice that would normally have been a gate question is recorded as a
decision with its alternatives, for him to overrule. Nothing merges; the work
is one branch and one pull request.

The decision itself is not re-litigated here. It was made on evidence
gathered the same day (branch `research/terminal-core`, draft PR 793): a
four-core bake-off, Jev-checked research over fourteen candidates, a
twenty-pane performance run, a six-agent fluency test, and Parker's own
weights, sealed before the final runs. See
`reports/2026-09-25-core-swap-final-round.html` on that branch.

## Gates

| Gate | Document | State |
|---|---|---|
| 1 Product | `01-product.md` | self-approved |
| 2 Architecture | `02-architecture.md` | self-approved |
| 3 Program design | `03-program-design.md` | self-approved |
| 4 Vertical slices | `04-slices.md` | self-approved |

## Progress

- [x] Clean room: `~/Work/td-core-swap-rio`, branch `feat/core-swap-rio`, cut from `origin/main` at 62613cf, upstream unset so no bare push can reach main
- [x] Warm build: the main checkout's `app/target` copied, `cargo check --locked` green in 22 s
- [x] Baseline on untouched main: 1,870 unit tests pass (6 ignored), integration suites 9 + 2 + 4 + 8 + 73 (1 ignored) + 9 pass — the bar
- [x] Inventory of every alacritty_terminal dependency: `evidence/alacritty-inventory.md` (about 200 items, about 415 production call-site lines)
- [x] rio-vt embedding map: `evidence/rio-vt-map.md`, with the claims that shape a decision re-checked against the crate source (vte 0.15 exposes `sync_timeout`/`stop_sync`, so both cores can run under TD's own loop; the PTY and pump need no crate beyond rio-vt)

## Surprises & discoveries

- TD depends on alacritty for far more than the grid: its `EventLoop` owns
  every pane's PTY reader thread, parser pump and writer; `tty` spawns the
  shell; `FairMutex` is the lock the renderer and the reader share; and the
  replica (`socketpty.rs`) exists only to dress a socket as the PTY that loop
  expects, down to hard-coding two `pub(crate)` poll keys by value. rio-vt's
  `Crosswords` cannot be driven by alacritty's loop, so the swap necessarily
  gives TD its own read loop. That is the centre of the architecture, and it
  deletes the socket-PTY workaround rather than porting it.
- `grid_hash` is built from alacritty's own bit layouts (`Flags::bits()`,
  `TermMode::bits()`, `NamedColor as u32`), and a window refuses a host whose
  `PROTO_VERSION` differs. Parker's session host is long-lived and must not be
  restarted, so a new window will meet an old host. Bumping the protocol would
  make that window refuse his running session; changing the hash would make it
  re-snapshot forever. Both are avoided by freezing alacritty 0.26's numeric
  layout as TD's own canonical encoding (see Decisions).
- alacritty's `Pty::on_resize` calls `process::exit(1)` when `TIOCSWINSZ` fails,
  which in a session host would end every pane at once. TD's own PTY layer must
  report the error instead.

## Decisions

Each is argued, with what it was chosen over, in `02-architecture.md`.

1. TD reads every core through its own boundary, `app/src/vt/`; nothing else names a core crate, and a test enforces it.
2. TD owns the read loop, the PTY and the lock. alacritty's `EventLoop`, `tty` and `FairMutex` leave; `socketpty.rs`'s workarounds are deleted rather than ported.
3. The parser sits inside the terminal lock, so a snapshot flushes an open synchronized update first.
4. alacritty 0.26's numbering of flags, modes and named colours is frozen as TD's wire encoding; the host protocol stays at version 1, so a new window still attaches to a running host.
5. alacritty stays as a non-default `core-alacritty` feature until rio-vt has held in daily use, then a follow-up removes it.
6. rio-vt runs with grapheme clustering off, matching alacritty's widths.
7. Pictures are attentional: dropped when their pane is hidden, never in a snapshot.

## Outcomes & retrospective

(At the end: the honest post-hoc difficulty, what the score predicted, and
what it missed.)
