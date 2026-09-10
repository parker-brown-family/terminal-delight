# Glossary

Words this repo uses that read like plain English but carry a specific meaning.
Written for a person skimming a report and for an agent arriving cold; when a
term names a decision, the pointer goes to the doc that decided it. Add new
entries when a reviewer or a report reaches for a word twice — the second use
is the sign it has become vernacular.

## The split (client-server feature, PR #328)

**session host / host** — The gpui-free process (`terminal-delight serve
--session <key>`) that owns the real terminals: PTY masters, child process
trees, the authoritative alacritty `Term`s, and scrollback. One per session,
reachable over a per-session unix socket. It survives the window; that is the
feature. It ends only on the `shutdown` verb or after ~12 idle hours with no
client attached and no pane output — checkpointing first (decided 2026-09-09,
`docs/plans/client-server/02-architecture.md`, "Host lifetime").

**client / replica** — The GUI window process after the split. It renders a
replica `Term` fed by the host's byte stream; selection, scroll, and every
render-path read stay client-side. A client dying kills nothing.

**hello** — The first thing a client is supposed to say on a control
connection: `{"verb":"hello","proto":<version>,"kind":"window"|"tool"}`. It is
the protocol negotiation — the host answers with its own version, and a
mismatch is how both sides discover that a binary swap made them incompatible
(see *stand down*). The `kind` says who is asking: a **window** is a GUI that
will render panes and participates in *steal*; a **tool** is a script or
automation and never steals. "Hello is optional" (the 2026-09-09 review's
first High finding) means the host currently answers any verb from a
connection that never negotiated — the fix makes hello a precondition.

**steal** — What happens when a second GUI window says hello to a host that
already has one attached: the newcomer wins, the host drops the previous
window's connections, the loser freezes (no new UI in v1). Named after tmux,
where a new `tmux attach` takes the session with it. Decided at Gate 2
(`02-architecture.md:222`), designed at `03-program-design.md:706-752` (the
`gui_conn` slot). As of 2026-09-10 it is **designed, not built** — the host
supersedes per *pane* (see *serial*) but has no window-level policy, which is
the review's second High finding.

**attach / the attach fence** — Attaching is asking the host for a pane:
snapshot first, then the live byte stream. The fence is how the host hands
both over without losing or doubling a byte: it takes the terminal's
`FairMutex` **lease**, then the data lock via `lock_unfair()` — the same order
as alacritty's own reader, and the reason a plain `lock()` deadlocks. Holding
the lease blocks a new PTY read cycle, so every byte lands in either the
snapshot or the stream, never both, never neither. Re-verify on any alacritty
upgrade.

**snapshot / gridwire** — `app/src/gridwire.rs`: reads a `Term` read-only and
emits the VT bytes that repaint a cold client, including the alternate-screen
case (a running vim). *Heal on exit* means the encoder never toggles the live
terminal's modes to do it — the original double-swap design would have erased
the vim it was copying.

**tee / sink / serial** — The host writes PTY output to its own `Term` *and*
tees it to every attached client sink. Each attachment gets a monotonically
increasing **serial**; a detach only releases the stream if its serial is
still current, so a superseded client's cleanup cannot kill its successor's
stream (a stolen stream and a dead shell both arrive as EOF — the serial is
how the host tells them apart).

**control connection vs byte stream** — Two socket connections per client:
control is NDJSON, one verb per line (hello, list-panes, spawn-pane,
attach-pane, resize, close-pane, save, shutdown); the byte stream opens with
`stream <pane_id>` and then carries raw terminal bytes both ways. Resize never
rides the byte stream. Contract: `docs/protocol/session-host-v1.md`,
conformance-tested.

**serverless path / `spawn_in`** — The original in-process spawn, kept
forever: scratch windows, demos, seeded tests, and the opt-out all use it.
"Serverless" here means *no host process*, not a cloud word.

**pane cap** — The host refuses `spawn-pane` past 64 (Gate 3's number). The
GUI's own 4-pane grid limit protects only the GUI; the cap protects the box
from a looping script. As of 2026-09-10: specified, not yet enforced (review
finding three).

**durable pane id / `TD_SESSION` / `TD_PANE_ID`** — The host mints a stable id
per pane and stamps both variables into every PTY it spawns. Tools read the
env instead of guessing from pids; shell pid becomes a reported attribute, not
an address.

## Recovery and failure words

**floor / TOML floor / respawn-and-type** — The pre-split recovery that
everything degrades to: the session TOML plus resume lines, replayed into
fresh panes at next launch. "Degrades to the floor" = you lose live processes
but not the session's shape or the agents' resume chain. Kept intact and
tested on purpose; host death lands here.

**checkpoint** — The host writing the session TOML now (atomic writer, shrink
guard, `.last-good`, rotated backups) so the floor is current. Done before any
planned exit.

**stand down / version-break handoff** — On a protocol-version mismatch at
hello, the old host checkpoints and exits so the new binary takes over from
the floor. Degrades to exactly today, once, deliberately. Exists to prevent
*two writers*.

**two writers** — The failure where a serverless window and a live host both
own one session file and overwrite each other's saves. Found by the second
review; the stand-down handoff is the fix.

**phantom window** — The incident class (#312, #314, the 2026-09-08 morning)
where a CLI call that should be invisible boots a GUI window. Killed by the
dispatch allowlist: unknown positionals exit 2 instead of falling through to
a launch.

**close clock / held pane / close-undo** — Parker's 2026-09-09 amendment
(`01-product.md`, "What closing means"): a manual close defers the kill — one
pane held an hour, two or more held four, ten most recent, processes running
the whole time, ctrl+shift+z to reopen. A **held** pane is one in that window;
distinct from a pane whose child exited on its own.

## Measurement words

**the metric** — Lost sessions = 0. The whole feature's success test, from
Gate 1: kill and relaunch the GUI N times during live work; everything is
still there and still running afterward.

**harness legs: gui-kill / floor-control / host-kill** —
`scripts/td-survival-test.sh`. *gui-kill* runs the metric (80/80 cycles kept
everything across 4 runs). *floor-control* deliberately loses (20/20) to prove
the instrument can see loss. *host-kill* kills the host instead, proving the
degrade to the floor.

**realistic flood vs saturating flood** — Bench loads in
`scripts/td-echo-bench.sh`. Realistic: 8 panes at ~2,000 lines/s each — chosen
to be heavier than a talkative build. Saturating: `yes` at full rate, which
pins every core and measures the box, not the terminal. The **flip gate** uses
the realistic one: 75µs p99 seam cost, 0 of 1000 keystrokes over 1ms.

**flip / flip gate** — Slice 5: hosted spawn becomes the default path (it is
opt-in until then). Gated on the measured numbers above plus Parker's
sign-off, recorded in `02-architecture.md` — never on a promise.

## Workflow words

**gates (1–4)** — The software-factory sequence every real feature runs:
Product → Architecture → Program Design → Vertical Slices, each approved
before the next. Docs live in `docs/plans/<feature>/`, state in
`00-status.md`. Approved gates are never redone unasked.

**slice / tracer slice** — A vertical cut that ships end to end. The tracer is
the first one, deliberately thin, proving the path before anything hangs off
it. Slice numbering can grow fractions (4.5) when review findings and
amendments land mid-feature.

**TPS report / decision brief** — A single self-contained HTML page under
`reports/`, annotatable in the browser; notes save into the file and export as
an anchor map, so "read my notes in \<file\>" replaces retyping reactions.
Decision-shaped work is delivered this way, never as chat scrollback.

**invalidation criterion** — Every claimed defect carries the check that would
prove it wrong, and whoever picks it up runs that check *first*. A finding
that survives its own invalidation test outranks one never subjected to it.

**prove the test can fail** — New tests run against the parent commit before
they count; a test that passes against the code it claims to catch is
decoration. Mutation testing is the same idea applied to a suite.

**one task, one pull request** — However many agents or attempts a task
consumed, it lands as one PR (#328 for this feature). Losing attempts keep
refs, not review slots.
