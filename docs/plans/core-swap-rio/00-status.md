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
- [x] Slice 1, TD's boundary and loop on alacritty: 1,875 unit tests and all six integration suites green, behaviour unchanged; `socketpty.rs` deleted; the boundary and frozen-numbering guards each fail when mutated
- [x] Slice 2, rio-vt as the core: 1,873 of 1,874 on the first run; the one failure (mouse protocols as one setting) fixed in the encoder; rio-vt's self-introduction replaced with TD's (`evidence/core-differences.md`)
- [x] Slice 3, pictures: drawn over the grid, forgotten when a tab is hidden; tests through rio-vt and through a real pane
- [x] Slice 4, real windows: hidden windows on their own sessions, photographed with the screens asleep; a probe's answers; a tab switch forgetting a picture; this build's window on a host from Parker's build. Found here and not by the suite: temporary-file and shared-memory pictures vanished in hosted panes, fixed by `picturewire.rs`, whose test fails without it
- [x] Slice 5, the words: README, glossary, licences, contributing, PR template, feature pages, protocol, security note, changelog, a Pictures page on the docs site; Part 5 of the Core Swap series filled with results (parker-dev, unpublished)
- [x] Latency: the echo bench, two alternating rounds on the branch and its base plus one with nineteen flooding panes (numbers below)

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

## Measurements

Echo bench, 1,000 samples a run, p50 / p99 in microseconds, base (`62613cf`,
alacritty) against this branch (rio-vt), nothing else running:

| Pane, load | base round 1 | base round 2 | branch round 1 | branch round 2 |
|---|---|---|---|---|
| window-owned, quiet | 30 / 43 | 30 / 43 | 24 / 39 | 25 / 42 |
| hosted, quiet | 67 / 126 | 67 / 103 | 56 / 83 | 54 / 112 |
| window-owned, 8 busy | 30 / 43 | 29 / 40 | 25 / 37 | 24 / 36 |
| hosted, 8 busy | 68 / 137 | 70 / 116 | 53 / 81 | 55 / 105 |
| window-owned, 8 saturating | 32 / 177 | 34 / 815 | 27 / 471 | 30 / 1,724 |
| hosted, 8 saturating | 116 / 3,477 | 132 / 4,557 | 106 / 3,222 | 105 / 3,067 |

With nineteen flooding panes (twenty in all), one round: window-owned 30 / 47
against 25 / 39, hosted 68 / 136 against 53 / 81; saturating window-owned
42 / 2,099 against 39 / 3,037, hosted 326 / 12,631 against 213 / 8,541. Both
builds pass the gate. The window-owned tail under a saturating flood is higher on
the branch in all three saturating runs; the cause is not established (a
follow-up issue).

Reading a full 10,030-line history through the boundary (release, best of
five): hash 29.6 ms rio-vt, 26.5 ms alacritty; snapshot 25.7 ms, 23.4 ms.

### Guard throughput

Measured after the merge, on 2026-09-25 (#843). The review put three guards in
front of rio-vt's parser (`vt/kitty.rs`, `vt/text.rs`, `vt/compat.rs`), and
each reads every byte of output before the core does. Nobody had timed them.
`what_the_guards_cost` in `vt/rio.rs` does: 8 MiB of each kind of output in
4 KiB reads at 120 by 40, the median of seven rounds, with the core alone timed
first and last in every round.

As merged, the guards read a byte at a time and took about a third of the
core's rate. Coloured text ran at 138 MiB/s through the core alone and at 88
through `advance`, slower than the bake-off had measured the bare crate (97).
Searching with memchr, reading printable ASCII eight bytes at a time and
decoding a character whole gave half of it back:

| Output, 8 MiB | Core alone, MiB/s | `advance` as merged | `advance` now |
|---|---|---|---|
| The bake-off's own `text-8mb.bin` | 135–140 | 87 (−35%) | 115 (−18%) |
| Coloured text, the bake-off's mix | 138–143 | 88 (−36%) | 114 (−18%) |
| ASCII only | 216–267 | 148 (−40%) | 188 (−13%) |
| Mostly not ASCII: CJK, box drawing, marks | 112–117 | 65 (−45%) | 72 (−35%) |
| Full-screen redraws in synchronized updates | 129–139 | 77 (−40%) | 107 (−23%) |
| Pictures, base64 in 4 KiB pieces | 613–663 | 311 (−53%) | 498 (−19%) |

The core alone moved by up to 8% between its two timings in one round, so a
smaller difference is noise. The bake-off's harness, run the same day, reads
`text-8mb.bin` at 103–109 MiB/s with rio-vt's defaults; TD's core is faster
alone because grapheme clustering is off. What is left is the text guard
reading every character and every control sequence. Each upstream fix in issue
836 lets TD delete part of it.

## Outcomes & retrospective

**Post-hoc difficulty: 8/10**, against 9 predicted. The architecture held without
a change of shape: every slice landed on the design in the gates, and the boundary
made each difference between the cores a local fix in one adapter. What kept it
near the top of the scale was the class of problem the score predicted — things
that look right and are wrong — and each of them was caught by a second reader:
the old suite run on the new core (the mouse protocols), reading rio-vt's source
for questions the suite never asked (how it introduces itself), and a real window
on a real session host (a picture the host deleted before the window could read
it). The last one no test would have found, because every test fed the replica
the same bytes directly; only the two-process arrangement a person actually runs
exposed it.

What the score missed: none of the gates was approved by Parker, because he was
away. Every gate says so and records its alternatives, so the first review is of
the whole design at once.

## Review, before merge (2026-09-25)

The review measured the swap against the host it has to live beside: the same
94 screens hashed by the build before the swap and by this one on both cores,
plus three second reviewers with probes that fed both cores the same bytes. The
design held. What it found was rio-vt's, and the boundary took every fix without
a patch to the crate. That is the adapter's argument, made a second time:

- **Blanks after a scroll, clear or erase read as alacritty wrote them**
  (`blank_background`). Before this fix, rio-vt parted from the old host on
  every such screen. Now it agrees on 92 of 94, pinned against the old build's
  own hashes (`gridwire`'s `AS_THE_OLD_HOST_HASHED`).
- **Three denial-of-service paths are stopped before rio-vt's parser:**
  quadratic combining marks (thirteen bytes took 9.7 s and 2.8 GB); an
  unterminated picture command (837 MB); and a picture read from a FIFO, which
  held the lock and, in the host, every pane's keystrokes behind it
  (`vt/text.rs`, `vt/kitty.rs`, and `Host::pane`).
- **Temporary picture files are deleted by kitty's rule, not rio-vt's
  substring test**, which let a path with `..` reach any file.
- **Seen on screen:**
  - synchronized updates no longer tear;
  - hiding a tab forgets both screens' pictures and gives their memory back;
  - a pre-swap host's snapshot keeps click and drag mouse reporting in a new
    window (`vt/compat.rs`).
- **The read loop ends a pane's life** however the child's status is lost.
- **CI now builds, lints and tests the alacritty fallback.**

What is left is listed in `evidence/core-differences.md` under "Left as they
are, knowingly". None is worse than one repair by the divergence guard.
Follow-ups: issue 836, reporting rio-vt's findings upstream and bounding the
two TD does not guard; issue 837, a closed pane that outlives its close; issue
838, an attach partway through a string; issue 839, copying and origin mode.
