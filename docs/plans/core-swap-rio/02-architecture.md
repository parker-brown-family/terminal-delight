# Gate 2 — Architecture: a core boundary TD owns

**State:** self-approved 2026-09-25 under Parker's instruction. Built from two
pieces of evidence in this folder — `evidence/alacritty-inventory.md` (what TD
uses today) and `evidence/rio-vt-map.md` (what rio-vt offers) — and every
decision below lists what it was chosen over.

## The situation the design answers

Terminal Delight is not built on alacritty's *screen* so much as on alacritty's
*machinery*. Its `EventLoop` owns each pane's reader thread, parser and writer;
its `tty` forks the shell; its `FairMutex` is the lock the renderer and the
reader share, and the session host's attach fence is built out of that mutex's
"lease". The replica that lets a window attach to a host-owned pane exists only
to dress a socket as the PTY alacritty's loop expects, down to naming two
private poll keys by value. About 415 lines of production code name alacritty
types, 227 of them in `pane.rs`.

rio-vt cannot be dropped into that machinery: alacritty's loop only drives
alacritty's `Term`. rio-vt has its own loop (`Machine`), but adopting it would
trade one core's private timing for another's — the same lease semantics
copied verbatim, a parser TD cannot reach when it needs to flush a
synchronized update before a snapshot, and a socket that has to be ported to
a mio 0.6 fork.

So the swap is really two changes, and the second one is the design:

1. the emulator becomes rio-vt, and
2. **everything around the emulator becomes TD's own** — the read loop, the
   PTY, the lock and fence, and the types the rest of TD reads.

## The shape

```
 main.rs · pane.rs · host.rs · gridwire.rs · pane/*      TD's application
        │  reads cells, modes, cursor, selection, pictures
        │  in TD's own types; never names a core crate
        ▼
 ┌──────────────────────────── app/src/vt/ ─────────────────────────────┐
 │ types.rs   Line Column Point Side  Color NamedColor Rgb               │
 │            Flags Modes Cell Cursor  Scroll SelectionKind  Event       │
 │            Picture                  (numbering frozen = alacritty 0.26)│
 │ term.rs    Terminal: one backend + its parser, behind one lock        │
 │ pump.rs    TD's read loop: poll a Source, read, [tee], parse, notify, │
 │            write queued input, watch the sync deadline, report exit   │
 │ source.rs  Source::Pty(master fd) | Source::Socket(stream)            │
 │ pty.rs     open a PTY, fork the shell, resize without ever exiting    │
 │ backend/   rio.rs        (default)    alacritty.rs (feature fallback) │
 └──────────────────────────────────────────────────────────────────────┘
        │
        ▼
   rio-vt 0.5.28  (default-features = false, features = ["graphics"])
```

Nothing above the line imports `rio_vt` or `alacritty_terminal`. A guard test
greps for it, so the boundary cannot erode quietly.

## The components

**`vt::types` — TD's vocabulary.** The things TD reads, in TD's own types:
positions (`Line`, `Column`, `Point`, `Side`), colours (`Color`, `NamedColor`,
`Rgb`), cell attributes (`Flags`), terminal modes (`Modes`), a `Cell`, the
`Cursor`, scroll requests, selection kinds, the events TD handles, and a
`Picture`. The numeric layouts of `Flags`, `Modes` and `NamedColor` are
**frozen to alacritty 0.26's**, pinned bit by bit in a test, because they are
TD's wire format: `grid_hash` hashes them, and a window running this build
must agree with a session host that is still running the last one.

**`vt::Terminal` — one emulator, one parser, one lock.** It wraps the active
backend's terminal *and its parser* in a single `parking_lot::Mutex`. Readers
(the renderer, the snapshot encoder, the hash) take the lock and read through
TD's types. The pump takes it to parse. Keeping the parser inside the lock is
what lets a snapshot say "flush any open synchronized update first", which
neither core's own loop allows.

**`vt::pump` — TD's read loop.** One thread per terminal, over `polling`
(already a dependency). It reads a `Source` *outside* the lock, then under the
lock: copies the chunk to an attached client if there is one (host), counts it
(replica), parses it, bumps the content generation, and posts a redraw. It
writes queued input when the source is writable, sleeps no longer than the
parser's synchronized-update deadline and flushes it when it passes, and ends
on end-of-file, `EIO`, or a shutdown message — reporting exit exactly once.

**`vt::pty` — the PTY, owned.** `rustix-openpty` to open it, `std::process`
with `setsid` and `TIOCSCTTY` to start the shell in it, the same environment
TD sets today, and a resize that returns an error rather than calling
`process::exit` as alacritty's does.

**`vt::backend::rio` — the default core.** rio-vt with its `graphics` feature,
no `pty` feature, a `Dimensions` that carries real cell pixels, grapheme
clustering off so widths match alacritty, and a listener that only posts
messages. It translates rio's cells, styles, flags, modes and events into
TD's types.

**`vt::backend::alacritty` — the fallback core.** The same boundary over
alacritty_terminal, behind a non-default cargo feature. It exists for two
reasons: the refactor is proven on it first, with every existing test green,
before any behaviour changes; and if rio-vt shows a problem in daily use, a
fallback build is one flag away instead of one revert away. It is removed by
a follow-up once rio-vt has held for a few weeks.

## The lock and the attach fence, made plain

Today the host's fence is "take alacritty's lease, then its *unfair* lock" —
correct only because alacritty's reader holds the lease across a whole read and
parse, and deadlock-prone because the fair lock would take the lease again. The
file that implements it records the day that went wrong.

With the pump in TD's hands the fence is one ordinary lock. The pump reads a
chunk, then, holding the terminal's lock, tees it, counts it and parses it. A
host attaching a client takes the same lock, flushes any open synchronized
update, encodes the snapshot, and installs the client's sink. Every byte is
therefore either parsed before the snapshot (and in it) or teed after the sink
was installed (and sent live). Nothing else to remember.

## Pictures

rio-vt stores Kitty pictures and their placements; TD draws them. Each frame a
visible pane asks its terminal for the placements that intersect the viewport,
already converted to viewport rows and cells, with the decoded pixels shared
behind an `Arc`, and draws them over the grid through gpui. Pictures are
**attentional**: when a pane's tab stops being shown, TD tells the terminal to
forget its pictures and drops the textures, so a picture is gone when you come
back. Snapshots carry no pictures, by the same rule. Placeholder cells
(`U+10EEEE`) remain text in this change; resolving them is a later slice.

## Compatibility

- **Old host, new window.** The host protocol stays at version 1, so a new
  window attaches to a running host without refusal. The snapshot is VT bytes
  and needs nothing. The divergence probe's hash is computed over TD's frozen
  encoding, which equals alacritty's, so identical screens agree. Where rio-vt
  and alacritty genuinely lay a screen out differently, the existing guard
  sees a mismatch and re-snapshots once — the behaviour it already has.
- **`TERM`** stays what TD sets today: `alacritty` when that terminfo exists,
  else `xterm-256color`, with `COLORTERM=truecolor`.
- **The terminal-input security audit** counts literal `notifier.notify(`
  calls. TD's own `Notifier` keeps that method name, so the audit keeps
  counting the same writes.

## Decisions

1. **A TD-owned boundary (`vt`) instead of rio types throughout.** Chosen over
   importing rio-vt directly into pane, host and gridwire. Costs an adapter;
   buys a core that can be swapped again by rewriting one module, a
   pre-1.0 API confined to one file, and a hash format that does not move
   with a crate's bit layout.
2. **TD owns the read loop** instead of adopting rio's `Machine` or keeping
   alacritty's `EventLoop`. The loop is where TD's own guarantees live —
   the tee, the fence, the byte count, the generation — so it belongs to TD.
   It also deletes `socketpty.rs`'s workarounds instead of porting them.
3. **The parser lives inside the terminal lock**, so a snapshot can flush an
   open synchronized update. This closes a gap the alacritty design had: bytes
   counted and teed but still buffered in the parser when a snapshot was taken.
4. **TD owns PTY spawning** instead of `teletypewriter` or alacritty's `tty`.
   Both alternatives bring an event system TD no longer uses (corcovado,
   `polling` registration), and alacritty's resize exits the process on
   error.
5. **Freeze alacritty 0.26's numbering as TD's encoding** instead of bumping
   the host protocol. A bump would make a new window refuse Parker's running
   session host, and the host is the thing that must not be restarted.
6. **Keep alacritty as a non-default fallback feature**, then remove it by
   follow-up. Chosen over deleting it in this change (no fallback) and over
   keeping it forever (two cores to maintain).
7. **Grapheme clustering off by default** in rio-vt, matching alacritty's
   widths so the existing tests and a mixed host/window pair agree. Programs
   can still turn it on with DEC 2027. *Amended in review, 2026-09-25: they
   cannot. The cluster path shares the quadratic cost of combining marks, so
   the request is dropped at the boundary (`evidence/core-differences.md`).*
8. **Pictures are attentional:** forgotten when the pane is hidden, never in a
   snapshot. Parker's word, and his to overrule.

## Risks, and what answers each

| Risk | Answer |
|---|---|
| The refactor changes behaviour before the core changes | Step one runs the whole existing suite on the alacritty backend through the new boundary; nothing moves to rio-vt until that is green |
| rio-vt differs from alacritty where TD's tests look | The same suite then runs on rio-vt; each difference is fixed in the adapter or recorded as a deliberate change |
| The pump starves the renderer | The lock is held per chunk (at most 64 KiB, well under a millisecond at the measured 97 MB/s) and released fairly |
| Out-of-range rows return the wrong row in release builds | Every row access in the rio adapter clamps, with a test at each edge |
| A resize to zero is undefined behaviour in rio-vt | The boundary clamps to 2 columns and 1 line |
| rio-vt's `graphics` feature or cell size is wrong and pictures vanish silently | A test places a picture and asserts the placement exists |
| A mixed host and window disagree | The guard already re-snapshots once; the frozen encoding keeps identical screens identical |
