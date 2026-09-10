# Handoff — client-server split, slice 3 onward (2026-09-09)

## Status

Slices 0–2 **done, pushed, green**. Branch `client-server-split` in the worktree
`~/Work/td-client-server`, ten commits, open as **draft PR #328**. Working tree
clean, nothing ahead of origin. **Nothing is installed and nothing is merged** —
deliberately; see *Watch out*.

Suite: **614 tests green**, and stable — verified over sixteen consecutive runs
after a flake was tracked to its cause rather than retried.

The feature works today up to "a terminal exists without a window and survives
one." What does not exist yet is a Terminal Delight *window* attached to it.

## What's done, and how each was verified

| Slice | What landed | Verified by |
|---|---|---|
| 0 · dispatch allowlist (`3d50f96`) | An unknown word exits 2 instead of opening a window and writing session state (#314) | 5 unit tests + 4 running the real binary against a throwaway HOME. **Two of them fail against the parent commit**, where `sevre` reaches `open window` at main.rs:18569 |
| 1a · `gridwire` (`884faaf`) | A terminal's scrollback, screen, colours, wide chars, alt screen and cursor as VT bytes another terminal can eat; plus a cross-process grid hash | 12 round-trip tests, then **mutation-tested**: 7 deliberate breakages, 6 caught (2 only after the tests were strengthened), 1 provably equivalent |
| 1b · `socketpty` (`227ddf3`) | Alacritty's own event loop driven by a socket — the client half of the seam | 4 tests over a socket pair: output, keystrokes, told-not-assumed resize, hangup ends the reader rather than spinning it |
| 2 · the host (`a3f1227`) | `serve --session <key>`: owns PTYs, pane table with durable ids, session-keyed socket + peer-uid check, 7 verbs with truthful outcomes, lease-fenced handover, SIGHUP close, env stamping | 8 unit tests over real PTYs + **4 driving the shipped binary over a real socket**, one of which is the product promise: a terminal outliving the window watching it |
| — · the lab (`f8a16f6`, `7caa226`) | `scripts/td-host-lab.mjs` — drive the host by hand, isolated from the live machine | Run end to end by hand: real login shell, background process, client killed, work still alive, reattach shown what it missed |

## How to run and verify

Build (≈20 s incremental; the worktree has its own reflink-cloned cache, so it
does not contend with any other agent's build):

```bash
cd /home/parker/Work/td-client-server/app && cargo build
```
```bash
cd /home/parker/Work/td-client-server/app && cargo test
```

Drive it by hand — nothing here can touch a live Terminal Delight:

```bash
cd /home/parker/Work/td-client-server && node scripts/td-host-lab.mjs up
```
```bash
cd /home/parker/Work/td-client-server && node scripts/td-host-lab.mjs spawn /tmp
```
```bash
cd /home/parker/Work/td-client-server && node scripts/td-host-lab.mjs attach 1
```

Start something slow, press **ctrl-]** to detach, attach again: the work never
stopped and you are shown what you missed. `down` stops the host. Spawn in a
neutral directory — a shell plugin's interactive prompt (mise's "trust this
config?") will eat the first thing you type.

**Run the suite more than once.** Concurrency bugs here are not deterministic;
a single green run is not evidence. See #331.

## Not done / next

**Slice 3 — the GUI attaches (the trunk; one agent, start here).** Everything
else is a side branch. It is the first slice that touches code paths a daily
terminal runs, even behind `TD_SESSIOND=1`, so the risk profile changes here.
- `resolve_session` (instance.rs:299) gains a first-ranked tier: a live host
  socket with no window attached — "your work is still running" is what a
  relaunch should find.
- `term::attach_in` beside `spawn_in`, building a `Session` from a `SocketPty`.
  **This forces the held slice-1 decision**: a `Session` has a `shell_pid` and
  an attached pane has no local process. Make it `Option<u32>` — unknown is not
  zero — and absorb the ripple into pane.rs.
- Orphan adoption: any live host pane the loaded layout does not claim lands in
  a new tab, so a stale checkpoint can never lose a running pane.
- The divergence guard, using `gridwire::grid_hash` (already written and tested).
- `td-survival-test.sh` — **the metric**: kill the GUI N times during a live
  agent run, assert scrollback, live `vim`/`htop`, tabs and notes all survive.
  Build its floor-control leg too: run against today's path it must report
  losses == cycles, or the harness itself is broken.

**Slice 2's debts — DONE** (`cb51542`, `f99d338`, `6347cd3` on
`client-server-split`; 631 tests green over eleven runs).
- `docs/protocol/session-host-v1.md` is the contract, and nine tests in
  hostproto.rs read it: every `json` example deserialises into the type its
  fence declares, no example may name a field the host would not write back,
  every verb and reply serde knows must appear on the page, every field the
  host emits must appear on it, and the version number and mismatch wording are
  the code's own words. It has already caught a real change — adding
  `grid-check` failed the suite until the page described it.
- The 800 ms watcher and the 30 s checkpoint now run in the host. Their answers
  ride `list-panes` as `mode`, `resume`, and a `cwd` that is a live reading
  rather than the spawn argument. All three are `None` until the host has
  looked: `classify_foreground` answers `Option` where pane.rs answers `Shell`,
  and a failed reading never overwrites a real one.
- Detached backoff: 5 s and 5 min with nobody watching; an attach or a spawn
  rings a bell both clocks sleep on, so cadence is restored at once rather than
  at the end of a five-minute sleep.
- Added beyond the three, at the slice-3 agent's request: `grid-check`, which
  answers with the authoritative grid's hash and the byte offset into the
  asking client's stream that it was taken at — both under the attach fence, so
  they describe one moment. It makes `gridwire::grid_hash` live rather than
  `#[allow(dead_code)]`.
- **Not moved, deliberately:** the session file is still written by the window.
  It carries window bounds, tab names and a theme no host has ever seen, so the
  host records rather than persists; the Save verb that hands the layout over
  belongs to the attaching client. Inventing a second TOML writer with no merge
  rule is what decision 15 of the program design warns about.
- **#335, found doing this and since fixed** (`4677b6e`): `serve` unlinked and
  rebound a live session's socket, stranding the host that still held its
  panes. It now claims the session with an flock beside the session files
  (`sessions/<key>.host.lock`) before touching the path, and only the holder of
  that claim ever removes or binds it. A second `serve` on a served session
  prints who is serving it and exits 0; on a held-but-silent one it names the
  holding process and exits 1, touching nothing. **The second refuses rather
  than taking over** — a host's value is pseudoterminals, which cannot move
  between processes, so a takeover would have to kill the work it exists to
  protect. Deliberately not `instance::claim_in`'s lock: that answers who may
  write the session file, which today is the window, and a host taking it would
  refuse to start for the window that asked for it. Slice 3's client-side
  `hostspawn.lock` was standing in for this and can go whenever its author
  wants.

**Open issues (all `follow-up` labelled, so they appear in `~/FOLLOWUPS.md`).**
- **#331** — a forked pane can hold a session lock its parent just released. The
  test symptom is fixed; *the production question is open* and is the one worth
  someone's afternoon.
- **#325** — the socket PTY hardcodes alacritty's crate-private dispatch tokens.
  Confirmed 0.26.0 is the newest published, so the upstream patch is real work,
  not a version bump. Parker concurred with sending it.
- **#327** — the phantom-window fix is built but not running. Filed deliberately
  as possibly-nothing, with the conditions that would escalate it.
- **#319** — the left-bar / tabs-as-tasks overhaul. Highest priority, entirely
  separate feature, pinned.

## Watch out

- **Do not install.** `~/.local/bin/terminal-delight` points at another agent's
  build (`td-1319a8b-no-keepalive` as of this writing). Installing from this
  branch would silently remove their work. That is #327's whole subject.
- **Do not build in `~/Work/terminal-delight`.** It is another agent's worktree
  with uncommitted work on `shorts-pipeline`. This machine currently has **six**
  terminal-delight worktrees plus six throwaway ones under `/tmp`; check
  `git worktree list` before assuming which tree you are in.
- **Never the stale fork** at `~/BROWN-FAMILY-SPORTS/Software/terminal-delight`.
- **One PR, not many.** Parker's instruction: a single pull request for the whole
  task so a rollback is one revert. Slices land as commits on this branch.
- **Tests that fork and tests that flock must not overlap** — `src/testsync.rs`
  explains why and guards it. If you add a test that starts a process, take the
  guard across the spawn.
- The pane cap decision is binding: **new splits stop at 4**, but a legacy layout
  holding up to 8 must still load with every pane. Only new splits enforce it.

## Three things the plan got wrong, found by building

Worth reading before slice 3, because they are the argument for building the
risky seam before wiring anything around it:

1. **The plan's locking deadlocks.** It said to pair alacritty's lease with the
   terminal's lock; the fair lock takes the lease too. Pair a lease with
   `lock_unfair` — which is what alacritty's own reader does.
2. **A superseded client used to take its successor's stream down with it.**
   Fixed with a serial on each attachment. Invisible to any test that attaches
   once; found by the end-to-end test.
3. **A hung-up socket is never read**, so the planned EOF detection could not
   have worked — the event loop skips a descriptor with the interrupt flag set.
   Hangup is established by peeking a second registration instead.

## In flight, as of 2026-09-09

Two agents are working from this document right now, in their own worktrees so
they do not share a git index or a cargo build lock:

| Agent | Branch | Worktree | Owns |
|---|---|---|---|
| A | `cs/slice-3` | `~/Work/td-cs-slice3` | the client side — `instance.rs`, `term.rs`, `pane.rs`, `main.rs`, `scripts/td-survival-test.sh` |
| B | `cs/host-debts` | `~/Work/td-cs-hostdebts` | the host side — `host.rs`, `hostproto.rs`, `docs/protocol/session-host-v1.md` |

Both branched from `client-server-split` at `51f44cf` and **merge back into it**
— there is one pull request for this task (#328), so a rollback stays one
revert. Neither opens a second PR.

Their scopes are disjoint except for one edge: **`main.rs`** is touched by slice
3 and was already touched by slice 2's dispatch work. A collision there will be
textual rather than semantic.

Whoever reads this next: check `git log --oneline client-server-split..cs/slice-3`
and `..cs/host-debts` before assuming either is unstarted.

## Where it's recorded

- Gate docs (the source of truth): `docs/plans/client-server/` — 00-status,
  01-product, 02-architecture, 03-program-design, 04-slices, plus the three
  research/panel records.
- PR: parker-brown-family/terminal-delight#328 (draft).
- Reports: `reports/2026-09-08-client-server-gate{1,2,3}.html` and
  `reports/2026-09-08-client-server-build-report.html` — the annotated ones
  carry Parker's own notes.
- APES episode: `2026-09-09-client-server-host.md` (terminal-delight).
- Memory: `~/.claude/projects/…-terminal-delight/memory/client-server-split.md`.
