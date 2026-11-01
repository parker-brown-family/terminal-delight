# Glossary

Words this repo uses that read like plain English but carry a specific meaning.
Written for a person skimming a report and for an agent arriving cold; when a
term names a decision, the pointer goes to the doc that decided it. Add new
entries when a reviewer or a report reaches for a word twice — the second use
is the sign it has become vernacular.

## The product (words a person meets in the app and the docs)

These are the words the docs site uses, with the page that describes each.
Entries marked **0.5.0** exist only on `dev/0.5.0` until that release ships.

**theme / design / skin** — Three separate axes of the look. A **theme** is
the colours: the painter's THEME area shows one tile for every theme Omarchy's
own picker shows, and **INHERIT** follows whichever the desktop is wearing. A
**design** is the finish rather than the hue: how much CRT texture and glow,
and which font (0.4.0 ships quiet, tactical, deco, hacker and intrusive, plus
`custom`, your own `theme.toml`; 0.5.0 ships Clean, deco, hacker and intrusive).
A **skin** is the chrome's shape: materials, lettering, corners, glass and
sticky-note paper (OPS, Chalk, your own `skin.toml`; 0.5.0 adds TERMINIMALIST).
**0.5.0:** design and skin move independently, and each design remembers its
**look dials** (the tuning you gave it) and its UI CONFIG choice. Pages:
`/themes`, `/skins`, `/ui-config`.

**the painter / GAUGES** — The painter is the overlay where the look is picked,
in three areas (Theme, Style, Rules). GAUGES is its panel of sliders; the CRT is
a switch at its foot.

**floating square / document square** — A document opened inside the terminal
(Alt+click on a path), drawn beside the line that printed it. A **clip** is a
video or sound played over the square once. Page: `/documents`.

**the workbench / bench / TERM ⇄ BENCH** — A pane's second face, where an
agent's replies, questions and handovers arrive as cards. A **surface** is one
such work object, and **TDSP** is the protocol an agent writes it in
(`docs/spec/td-surface-protocol.md`). Pages: `/workbench`, `/surface-protocol`.

**handover / artifact** — Something an agent handed you: a page, a file, a link.
The pane header names the newest one. **0.5.0:** the **artifact rail** (📜) lists
each agent's handovers, newest first, and its Archive keeps them after the pane
closes. Page: `/artifacts`.

**brief / notes / CONCUR** — A decision brief is an HTML page an agent makes for
you to annotate. Notes attach to its elements (and to a Markdown file's), a
CONCUR stamp records agreement, and **↪ send to agent** puts the notes in that
agent's prompt. Page: `/briefs`.

**off-limits** — A pane a person has hidden from every other agent, from its
right-click menu. An agent that wants more access has to ask (`request_access`)
and wait for a person's Yes. **0.5.0** adds **read-only** as a third state, and
`list_panes` reports a pane's **reach** where it said `exposed`. Pages: `/mcp`,
`/agent-to-agent`.

**Git signal / coordination** — The one compact signal that warns when panes
are changing the same files, and the inspector Alt+G opens. It observes and
never writes to a repository. Page: `/git-signal`.

**LOOKUP** — Alt+L: every project, group, tab, pane and sticky note in one list,
found by typing part of its name. Page: `/left-bar`.

**the call** — What an agent card's three bars (context, fatigue, relevance) add
up to: RUN, WATCH, COMPACT, HAND OFF or STOP. Only STOP and HAND OFF light the
card, because only those need a person. Page: `/agent-wall`.

**Threshing Floor (0.5.0)** — The org chart of a window's agents that decides
which agent may reach which. It was called the War Room until 2026-10-27; the
saved keys `war_orgs`/`war_org` and the ask record's legacy `warroom/` directory
keep the old name on purpose, so an older build reads the same files. Page:
`/threshing-floor`.

**org / seat / line / rung (0.5.0)** — An **org** is one group on the Floor. An
agent dragged into it has a **seat**. A **line** runs from one seated agent to
another, and its **rung** is how far the first may reach the second, each rung
including the ones below it: **see** (find the pane in `list_panes`), **read**
(its screen text, tool calls and appearance), **ask** (a question answered
without touching its conversation, `ask_agent`), **prompt** (a turn typed into
its conversation, `prompt_agent`). An agent with no seat is never asked.

**read-only copy (0.5.0)** — How an ask that the window cannot answer from what
it already shows gets answered: a copy of the other agent that can only read,
started only after a person says Yes.

**rest / sleep / wake (0.5.0)** — An agent pane untouched and quiet for ten
minutes **rests**: its processes are frozen and their memory pushed out. After
two hours of rest the agent **sleeps**: it is ended with its conversation kept
(Claude Code; Codex only when it runs its own server). Focusing a resting pane
**wakes** it where it was; a key or a click wakes a sleeping one into a fresh
pane. `TD_AGENT_REST=0` turns it all off. Page: `/rest`.

**Clean / UI CONFIG (0.5.0)** — Clean is the design with no tube and no light
anywhere, and the one first launch opens in. UI CONFIG is the painter section
that hides any piece of chrome, at an OUTER layer (the window) and one PANE
layer shared by every pane. Page: `/ui-config`.

**scripted window (0.5.0)** — A window started with `TD_MCP_SCRIPTED=1`. Only
such a window lets `ctl` change MCP policy or act as a person (`bench`, `doc`,
`tabs`, `adopt`); an ordinary window refuses. Page: `/ctl`.

**Restricted (0.5.0)** — `terminal-delight restricted run`: an offline command
with one project directory, a temporary home and no host services. Page:
`/restricted`.

**development markers** — On the docs site, `class="in-dev"`, a `dev-tag` span
or a nav.json `"release"` marks what exists only on the next release's branch.
See `docsite/ARTICLE-PROFILE.md`.

## The split (client-server feature, PR #328)

**session host / host** — The gpui-free process (`terminal-delight serve
--session <key>`) that owns the real terminals: PTY masters, child process
trees, the authoritative terminal cores, and scrollback. One per session,
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
connection that never negotiated. It is still optional by design: the host
answers every verb the same with or without a greeting, and the `answer_cold`
tests in `host.rs` hold the two paths to that.

**steal** — What would happen when a second GUI window says hello to a host
that already has one attached: the newcomer takes the session and the first
window loses its connections, as a new `tmux attach` does. It was designed at
Gate 2, built on 2026-09-10 and reversed the same day: nobody asked for it, the
authorisation boundary is the peer-uid check on the socket, and anything that
can open a second window on a session can already do worse than watch it. Two
windows now share a session's panes, and what arbitrates a pane is attaching to
it: the most recent attach wins that pane and every other pane stays where it
was (`two_windows_on_one_session_share_its_panes` in `host.rs`).

**attach / the attach fence** — Attaching is asking the host for a pane:
snapshot first, then the live byte stream. The fence is how the host hands
both over without losing or doubling a byte, and it is the terminal's own
lock: TD's read loop copies each chunk to the attached client and parses it in
one step under that lock, so a snapshot taken holding it lands between chunks,
and every byte is in either the snapshot or the stream, never both, never
neither. The fence also flushes an open synchronized update into the grid
before encoding. Until the core swap (2026-09-25) it was alacritty's
`FairMutex` lease paired with `lock_unfair()`.

**core / vt boundary** — The terminal emulator core turns a program's bytes
into a grid of cells. TD's is **rio-vt**, the core of the Rio terminal;
`alacritty_terminal` is a fallback built with `--features core-alacritty`.
Everything TD reads about a terminal goes through `app/src/vt/`, in TD's own
types with alacritty 0.26's numbering frozen (the divergence hash is built
from those numbers). Decided in `docs/plans/core-swap-rio/02-architecture.md`.

**pictures (attentional)** — Images a program draws with the Kitty graphics
protocol. They appear in the pane while it is looked at; when its tab is
hidden the pane drops the textures and the core forgets the images, and no
snapshot carries them. Parker's word for it was attentional.

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

**pane cap** — Two numbers. A person can split a tab into at most four panes
(`MAX_PANES` in `main.rs`); a layout saved before that cap, holding up to eight,
still loads whole (`LEGACY_PANE_CEILING`). Separately, the host refuses
`spawn-pane` past 64 live terminals (`HOST_PANE_SANITY_CAP` in `host.rs`), a
sanity cap that protects the machine from a script spawning in a loop rather
than a layout policy.

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
