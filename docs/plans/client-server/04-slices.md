# Slices: Client-server split

Approved 2026-09-08 ("Go full send"). Build order, one slice at a time, each
ending in a state that runs and can be shown. No horizontal building: no slice
is "all of the wire, then all of the host, then all of the GUI".

Every slice ends with: the suite green (`cargo test` in `app/`, 570 tests at
the baseline), the new behaviour demonstrated rather than asserted in prose,
and a commit whose message names the least-confident decisions it touched.

Work happens on `client-server-split` in the worktree `~/Work/td-client-server`
(cut from `main`), never in the live tree — another agent holds uncommitted
work there and its build is the one currently installed.

---

## Slice 0 — the dispatch allowlist · SHIPS ALONE

**What:** `Launch`/`Verb`/`dispatch` replace the if-ladder in `main`. A known
verb dispatches headless; a `-`-leading word goes to the existing flag gate; a
positional naming an existing directory opens a window *there*; anything else
exits 2 with usage. The pinning test that asserted the old fall-through is
rewritten in the same commit.

**Why first:** it is the whole of #314 and it needs nothing else in this
feature to exist. Today a typo'd subcommand opens a GUI window that adopts or
mints a session and writes to disk — the phantom-window class. This closes it
in one reviewable diff.

**Proof:** `terminal-delight sevre` exits 2 naming the word, opens no window,
and leaves no state files behind (asserted against a temp HOME by a real-binary
test, not only a unit test).

**Not in this slice:** the `serve` verb itself. Adding a verb whose handler
does not exist would be a lie in the allowlist; `serve` joins in slice 2, in
the commit that gives it a handler. Until then `terminal-delight serve` is
correctly refused as unknown.

## Slice 1 — the seam, with no behaviour change

**What:** `term::Session::attach_in` beside `spawn_in` (spawn split into
`spawn_pty` + `wire_event_loop`), the `SocketPty` adapter over alacritty's
public evented-PTY traits, `gridwire::encode_snapshot` + `grid_hash` with their
round-trip property tests, and `SavedNode.pane_id` (absent, never zero).

**Why here:** it is the risky mechanism, and it can be proven entirely in
tests before any process talks to any other process. The encoder is the piece
whose first design would have erased a running vim; it earns a property test
over the existing headless VTE matrix before it earns a socket.

**Proof:** round-trip tests — encode a Term carrying scrollback, SGR extremes,
wide/zerowidth cells, alt screen; replay into a fresh Term; assert cell-for-cell
equality and equal hashes. Nothing user-visible changes; the suite grows.

## Slice 2 — the host

**What:** the `serve` verb (now joining the allowlist), the pane table with
durable ids, the socket with its uid check, the nine control verbs with truthful
outcomes, the per-pane byte streams, the lease-fenced snapshot-then-tee splice,
SIGHUP close, `TD_SESSION`/`TD_PANE_ID` stamping, the relocated watcher and
checkpoint with their detached backoff, and the one-page protocol contract.

**Proof:** headless integration tests — a host with no GUI anywhere: spawn a
pane, write to it, read the tee'd bytes back, attach twice and watch the first
attach get dropped, close a pane and watch the child tree die, disconnect and
watch nothing die. The contract document's own examples must deserialize.

## Slice 3 — GUI attach, behind `TD_SESSIOND=1`

**What:** the live-host tier in session resolution, the attach path, the
replica wiring, the divergence guard, and the survival harness.

**Why the flag:** the default path stays untouched until the numbers say
otherwise. This is the slice where the two honest unknowns get measured —
dual-Term cost and input-echo latency.

**Proof:** `td-survival-test.sh gui-kill --cycles 20` green, plus its
floor-control self-test reading losses == cycles on today's path (a harness
that reports zero against the unfixed build is itself broken), plus the
host-kill leg diffing recovery against the recorded control.

## Slice 4 — persistence redirect and orphan adoption

**What:** the host becomes the single TOML writer; saves route to it as an
opaque schema-versioned envelope; any live host pane the loaded layout does not
claim is adopted into a new tab; `MAX_PANES` drops to 4 for new splits while
the loader still opens legacy layouts holding up to 8.

**Proof:** a hand-written legacy 8-pane session file loads with all eight panes
running; splitting that tab refuses at four.

## Slice 5 — flip the default

**What:** hosted boot becomes the normal path; `TD_SESSIOND` inverts into an
escape hatch.

**Gate:** input-echo p99 within 1 ms of today, no visible lag at eight panes
under a cat-flood, survival script green at N=20 including the host-kill leg —
measured numbers, signed by Parker, not a promise.

## Slice 6 — tie-off

The follow-up issues 03 commits to: the `grid.frame` structured-read door, the
push-feed gap, the write-gating unification, tear-off attach-by-id, per-pid
socket retirement. Filed falsifiably, never left in a doc.
