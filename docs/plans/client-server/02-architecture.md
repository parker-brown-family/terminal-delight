# Architecture: Client-server split

Drafted 2026-09-08 from a three-sketch design panel (three priors, three judge
lenses; scores 12.5 / 10.5 / 8, two lenses for the winner). The full record of
what was considered and rejected — including both losing sketches — is
`research-gate2-panel.md`. Awaiting Gate 2 approval; the five questions at the
bottom are the decision round.

**The shape:** one installed binary. A gpui-free, per-session "session host"
process (`terminal-delight serve --session <id>`) owns the PTY masters, child
process trees, alacritty EventLoops, authoritative `Term`s, and scrollback.
The GUI window becomes a mortal client that attaches: it drives a client-side
replica `Term` over a socket, so every existing read/write site in the render
path stays today's code. Closing a pane or tab sends a kill verb (intent);
client disconnect, crash, or binary swap kills nothing; host death degrades to
today's TOML respawn-and-type recovery, which is kept intact and tested.

## Fit

- `app/src/main.rs` — the dispatch ladder gains a `serve` verb and an
  allowlist: unknown positionals exit 2, killing the #314 phantom-window
  fall-through; the pinning test at main.rs:17172 is rewritten in the same
  commit. Ships as slice 0, before any host exists.
- `app/src/term.rs` — `Session` gains `attach_in` beside `spawn_in` (the
  seam, cut at construction — see Flow). PTY spawn code moves to the host
  verbatim; `spawn_in` is never deleted (scratch/demo/seeded windows keep the
  serverless path).
- `app/src/pane.rs` — untouched on the grid: all 33 `.term.lock()` sites
  (render, hit-test, selection, scroll, FOCUS) run against the replica.
- `app/src/instance.rs` — `claim_in`/`release` move to the host verbatim,
  fail-closed contract and release ordering intact. `resolve_session` gains
  one tier ranked first: a live host socket with no attached GUI.
- `app/src/session.rs` — `capture`, the 800ms mode watcher, and keepalive run
  in the host, where the PTYs live. The five-level `agent_resume` forensic
  chain survives unchanged; ledger-absent stays the operating mode.
- `app/src/gridwire.rs` — new: the attach-time snapshot encoder
  (grid → VT bytes), plus the divergence guard (grid hash host-vs-replica at
  generation quiescence; mismatch forces a loud re-snapshot).
- Persistence — `persist_primary_state` → `write_atomic` relocates so the
  host is the single TOML writer; shrink guard, `.last-good`, rotated backups
  intact.
- The per-window ctl socket, MCP core/relay, headless verbs, td-send, and
  tear-off env are unchanged. ctl Owning/relay resolution reads host-stamped
  `TD_PANE_ID` before falling back to the /proc parent-walk (shrinks #215).

## Endpoints

`$XDG_RUNTIME_DIR/terminal-delight/session-<id>.sock` — in the existing 0700
dir, keyed by session id, guarded by the flock (stale-socket and pid-recycling
hazards structurally gone). Auth is `SO_PEERCRED` uid check at accept and
nothing else; no protocol field carries identity.

- Control connection, NDJSON, one verb per line: `hello` (proto version,
  kind) · `list-panes` · `spawn-pane` → `{pane_id, shell_pid}` · `attach-pane`
  · `resize` · `close-pane` · `save` · `shutdown`; mode/exit events pushed.
  Writes report truthful per-target outcomes (the `UiReq::Apply` shape).
- Byte-stream connection, one per pane: first line `stream <pane_id>`, then
  raw bytes both ways. Resize never rides the byte stream.
- The dozen verbs live as a one-page versioned contract under
  `docs/protocol/`, conformance-tested against the document.
- Protocol-version break: graceful shutdown, then TOML recovery — degrades to
  exactly today, once, deliberately.

## Data

- Session TOML: same location, same atomic writer, host-owned. `SavedNode`
  gains `pane_id: Option<u64>` — absent means pre-split file; unknown is not
  zero.
- The layout body the host stores is an opaque, schema-versioned envelope —
  layout stays client-owned data, so the left-bar overhaul (#319) can add
  fields without a wire break and never grows a server dependency.
- The host mints durable pane ids and stamps `TD_SESSION` + `TD_PANE_ID` into
  every PTY it spawns; shell pid becomes a reported attribute, shrinking the
  #311/#299/#272/#151 cross-wiring class.
- Scrollback lives in host memory only, never on disk — by design; the host
  existing is what makes that survivable.

## Flow

Attach (the main path): GUI launch → `resolve_session` finds a live host
socket (or spawns a setsid-detached host) → `hello` → `list-panes` → per pane:
`attach-pane` + byte-stream connect. The host's attach handler takes
`FairMutex::lease()` then the data lock — the same order as alacritty's
`pty_read`, so no deadlock — snapshots the grid through `gridwire`, registers
the byte tee, releases both. Holding the lease blocks a new read cycle from
starting, so every byte is either in the snapshot or tee'd, never neither,
never both. The lease invariant is documented as the tripwire to re-verify on
any alacritty upgrade. Client side, `attach_in` runs the stock EventLoop over
a `SocketPty` adapter (alacritty's public `EventedReadWrite`/`EventedPty`
traits) into the replica Term; the generation counter stays the invalidation
token; selection and scroll remain client state by construction.

Input: keystroke → GUI → byte stream → host PTY. Close pane/tab →
`close-pane` verb → SIGHUP the child tree. App quit → disconnect only. Host
crash → today's respawn-and-type recovery, proven by the harness's host-kill
leg, not by argument.

## External

None. Env var names: `TD_SESSION`, `TD_PANE_ID` (host-stamped),
`TD_SESSIOND` (opt-in flag during migration), `XDG_RUNTIME_DIR`. systemd
--user supervision is a follow-up, not a dependency (substrate verified:
KillUserProcesses=no, Linger=yes — setsid hosts already survive logout).

## Settled by the panel

Agreement across all three independent priors (each line settled input, not
up for re-litigation): server owns PTYs/Terms/scrollback and the GUI is
mortal; nothing existing is deleted (spawn_in, recover.rs, forensics);
close-is-intent implemented literally; flock moves with the PTYs; durable
pane ids replace pid addressing; the three multi-attach non-assumptions hold
by construction; auth stays a transport fact; #314 dies in-feature; the wire
reuses NDJSON framing and truthful-outcome writes; tmux children are never
interpreted; per-pane kernel introspection moves to the host; Gate 1 approval
is recorded as the 2026-08-29 staged trigger formally firing.

Fork resolutions (divergence detail in `research-gate2-panel.md`): grid seam
= client-side replica, cut at `Session` construction (2–1, and the dissent's
byte-loss race was settled by the lease repair above — verified against the
vendored crate, a ~3-line fix); per-session hosts, not a per-uid daemon (one
crash loses one session to the TOML floor); one binary, not a client/server
pair (a split doubles the #312 stale-artifact class); write-gating unified on
the new surface only, #308 stays its own ticket; push-feed closure, per-pid
socket retirement, and server-side layout all refused as unfunded by the
metric — filed as follow-ups at tie-off.

## Measured gates before the default flips

The two honest unmeasured bets: dual-Term cost (2x parse on bulk output, 2x
grid memory) and input-echo latency (two extra unix hops). The kill-relaunch
harness (`td-survival-test.sh` shape: sentinel scrollback + live vim and htop,
kill -9 the GUI, relaunch, assert everything, N cycles, plus a host-kill leg)
is built at the opt-in stage; the flip is gated on its numbers, not on a
promise. The `grid.frame` structured-read door stays open post-v1 on the
authoritative Term without re-cutting the seam.

## Host lifetime — decided by Parker, 2026-09-09

**A host exits after a long idle: no client attached and no pane output for
~12 hours. It checkpoints before it goes, and says so in the log.** This gates
slice 5: the default does not flip until the policy is in.

Approved from the brief `reports/2026-09-09-what-ends-a-host.html` (three
questions, "concur" to each).

Why this rather than the alternatives, so it is not relitigated:

- **Nothing ends a host** was the behaviour, not a choice — the Gate 2 record
  asserted "exits when its last pane closes" and only the `shutdown` verb was
  ever built. Measured 2026-09-09: an empty host is 15 MB and immortal, cleared
  only by a reboot. Untenable once every window is hosted.
- **Exit on last pane** is what the design assumed, and it carries the
  restore trap flagged at Gate 3: a window that closes panes before it opens
  them takes its own host down mid-restore. It also makes a session
  unreattachable one second after you close its last pane, which is sometimes
  exactly what you wanted.
- **Bounded by the login session** trades away logout survival, which Gate 1
  named as a problem worth solving.
- **Prior art is unanimous and inapplicable.** tmux, screen and zellij never
  exit — but you *opt into* tmux, so a server that lives forever matches a
  promise the user made deliberately. Hosted-by-default is the opposite: the
  mechanism transfers, the policy does not.

**Generous on purpose.** Twelve hours needs no heuristic about whether a silent
agent is thinking, and a heuristic is what would eventually kill something
irreplaceable in a way nobody could reproduce. Two hard rules follow: **never
exit while a client is attached**, whatever the panes are printing — an
attached window is proof the session is wanted; and **checkpoint before
exiting**, so what remains on disk is fresh rather than hours stale.

## Flip gate — measured 2026-09-09

**The input-echo gate is met.** An attached keystroke costs **41–136µs** more at
p99 than a local one, against the 1000µs Gate 2 asked for. Three runs of
`scripts/td-echo-bench.sh`, release build, 1000 samples per condition:

| Run | Machine load | Quiet | 8 panes, realistic | 8 panes, saturating |
|---|---|---|---|---|
| 1 | 10.4 | 108µs | 134µs | 6537µs |
| 2 | 14.1 | 41µs | 81µs | 1180µs |
| 3 | 15.8 | 136µs | 53µs | 5422µs |
| 4 | after the 2026-09-10 rework | 39µs | **30µs** | 2741µs |

### Re-measured 2026-09-10, after the contract core and its reversal

Run 4 above is the same instrument on the branch as it stands after slice 4.5
and the reversal of two of its commits — the host now carries a sixty-four pane
cap, a bounded dead-pane table, a checkpoint on the way out, and a stateless
wire. **Both halves of the gate still pass, and the realistic figure is the
best of the four runs:**

- **Echo, realistic load, eight panes: 30µs** more at p99 than a local
  keystroke (local 62µs, attached 92µs), **0 of 1000 keystrokes over a
  millisecond.** Quiet: 39µs, also 0 of 1000.
- **Saturating: 2741µs, 42 of 1000 over a millisecond** — reported, not gated,
  and inside the 1180–6537µs band the same code has produced across four runs
  on different background loads. It remains a scheduler number.
- **Survival, gui-kill at N=20: 20 of 20 cycles lost nothing.** Four terminals
  a cycle — three thousand lines of scrollback, an nvim, a btop, an idle shell
  — the window killed outright each time, `"losses":0,"failures":[]`.

The floor-control leg was not re-run, and does not need to be: it measures
today's serverless path, which this rework did not touch, and its job is to
prove the instrument can see a loss at all. Its standing result is 20 of 20
lost. Nothing here is an eyeball check — no VISIBLE lag at eight panes is still
a person's judgement and still unrun.

Keystrokes over a millisecond, per thousand: **0–6 realistic, 23–189
saturating.** The instrument is identical across every condition, so the figure
is the cost of the seam and nothing else.

**The realistic load is the gate**, and it is eight panes each printing about
two thousand lines a second in bursts — `while :; do seq 1 200; sleep 0.1; done`
— which is heavier than a talkative build.

**The saturating column is not a gate and must not be read as one.** It is
`yes` at full rate on eight panes, which leaves a sixteen-core machine with no
spare core, and the attached path pays the shortage twice: two processes, two
terminals and two parsers for one stream. **It swung 5.5× across three runs of
identical code** — 1180µs to 6537µs — with nothing changing but background load,
which is what a scheduler-bound number does. The realistic column stayed inside
a narrow band across the same three runs, which is what a work-bound number
does. That contrast is the evidence that the tail belongs to the machine rather
than to the design.

It was measured from the other side too, and agrees: a host-side probe
(`where_a_keystroke_waits_inside_the_host`) timed an interval containing
**nothing at all** at 201µs p99 under the saturating flood and 2µs under the
realistic one. Taking the global pane table — the hypothesis that a lock sat on
the keystroke path — cost 147µs, less than the noise floor. It cannot be what
costs milliseconds.

**What these numbers do not cover**, stated so they are not read for more than
they are:

- The realistic load is a **stand-in chosen for being obviously heavier than a
  build, not a recording of one.** Nothing here measures where a real workload
  sits, only that one well above a build's output is comfortably inside.
- **One machine**, sixteen cores, under substantial external load (10–16) from
  other work. A smaller machine has less headroom, and the realistic case could
  approach saturation there — the run to repeat before shipping to one.
- **Keystroke to grid, not to pixels.** The bench waits on the replica's content
  generation, so rendering is outside it.

## Decision round — answered by Parker, 2026-09-08 (annotated brief)

1. Second attach: **steal, tmux-style** — the new attach wins, the host drops
   the previous client connection. Multi-attach stays a one-line policy
   change later.
2. Flip gates: **as recommended** — input-echo p99 within 1ms of today, no
   visible lag under a cat-flood, survival script green at N=20 cycles
   including the host-kill leg; Parker signs measured numbers, not a promise.
   (The flood measurement stays at 8 panes as a stress/legacy case even
   though the new layout cap is lower — see 5.)
3. Tear-off: **keeps today's respawn semantics in v1**; attach-by-id is a
   follow-up once pane ids exist.
4. Supervision: **bare setsid in v1**; a socket-activatable systemd --user
   template is a follow-up.
5. Pane cap — **amended, not the recommendation**: the per-window cap drops
   to **4**. Parker: "cap is 4... 8 is nonsense for one task" — a window
   holds one task's panes (the tabs-as-tasks direction, #319). The host takes
   only a generous sanity cap. Consequence for Gate 3: `MAX_PANES` (main.rs:75)
   becomes 4 for new layouts, and a legacy session file holding 5–8 panes
   must still load without losing a running pane — the orphan-adoption rule
   covers live panes; the loader must tolerate over-cap saved layouts.

### Amendment, 2026-09-10: decision 1 is deleted rather than built

Decision 1 above stands as the record of what was approved on 2026-09-08 and is
left visible for that reason. It is no longer what the code does, and it is not
going to be.

**What replaces it.** A second attach wins *that pane* and nothing else. Two
windows on one session share its panes: most recent attach wins per pane, every
other pane stays where it was, both windows stay live, and neither is frozen or
refused. `ClientKind` travels on the wire and gates nothing.

**Parker's reasoning, on the reconciled fresh-agent review.** Window-level steal
was approved at Gate 2, written out at `03-program-design.md:706-752`, and never
implemented — the host discarded the client kind outright for the whole life of
the branch. Two reviews filed that as a P1; the reconciled review took it the
other way, and the decision was to delete the policy rather than build it. No
user has asked for it. The authorisation boundary is the peer-uid check on the
socket, and anything that can open a second window on this session can already
kill the host outright, so what steal would arbitrate is not a security question
but a preference nobody has stated. Shared panes is also what the comparable
product does.

**What survives, and it is not the same thing.** Pane-level supersede stays: an
attachment carries a `serial`, and a stream ends when another client takes that
pane. That mechanism predates steal and something important depends on it — a
window whose stream ended asks the host whether the terminal exited or was taken
away, and reaping panes that are still running is a bug this branch has already
fixed once.

**The one thing this leaves open**, recorded so it is a decision rather than a
gap: attach is open to any connection, tool or window (issue 353). A same-uid
peer can already displace a pane's stream and can stop the host, so a check
there would be theatre rather than a boundary. Accepted for v1.

Built and reversed the same day, 2026-09-10 — the reversal is two commits on
this branch, and the shared-panes behaviour is asserted by
`two_windows_on_one_session_share_its_panes` rather than left implied. Issue 351
records the behaviour for whoever meets it next.
