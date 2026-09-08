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
