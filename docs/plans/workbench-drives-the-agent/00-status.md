# Status: The workbench drives the agent

**Difficulty: 9/10** — a new API boundary between the workbench and the agent
process, replacing a coupling that four existing subsystems assume. It is a
protocol, so being wrong is expensive for a long time and other things start
targeting it; and the failure it fixes is silent, which means today's tests say
nothing about whether the new one is correct. → **four gates.**

**Turned out to be:** _(filled in at the end)_

- Gate 1 — Product: **APPROVED 2026-09-21** — Parker, in chat: *"Great - love it
  yes."* Approved after the amendment below, which was folded in before the
  approval rather than after it.
- Gate 2 — Architecture: pending — **and it belongs to a different agent.**
  Parker: *"the async work will belong to another agent. Let's wrap up our WIP
  BEFORE we start the ASYSNC"*. This session's job ends at Gate 1 plus the
  capture.
- Gate 3 — Program Design: pending
- Gate 4 — Slice plan: pending

**AMENDED at approval, 2026-09-21 — the PTY side-channel.** Raised by the pane
building bench tenancy and verified here against `origin/main`. `bench_deliver`
writes raw bytes into the pane's pseudoterminal, and two gestures that are not
composing depend on it: answering a menu the agent is blocked on
(`Dispatch::Keys`), and ending the agent (two interrupts, deliberately not a
signal). It is synchronous with a terminal by nature. Gate 1 now carries the
three possible answers and names the choice as one to make on purpose rather
than discover at implementation. See `01-product.md`, "The third cost".

## Slices

_Not planned here. Gate 4 belongs to the agent taking the async work._

## For whoever picks up Gate 2

Read `01-product.md` whole, then these three, in this order:

1. **"The third cost"** — the PTY side-channel. It is the decision most likely
   to be made silently and most expensive to reverse, because option 2 removes a
   gesture people use today.
2. **"Constraints on Gate 2 that Gate 1 already fixes"** — the inbox stays
   pane-addressed. `$TD_SESSION` and `$TD_PANE_ID` are stamped at shell spawn,
   before an agent exists to name, so they cannot become conversation-keyed. The
   store may be re-keyed; the address may not.
3. **`docs/plans/workbench-follows-the-agent/`** — bench tenancy, Gate 1
   approved, Gate 2 in progress, a different agent. It answers *which
   conversation is this workbench showing*, which a decoupled workbench needs and
   would otherwise have to invent. Its proposed reader is `tenancy_for(pid,
   home)` / `tenancy_of(session_id, home)` returning a `Tenancy` enum rather than
   an `Option` — coordinate with that pane before defining a second boundary.

### The seam, measured on origin/main 2026-09-21

| What | Count |
|---|---|
| `self.bench` across `pane.rs`, `pane/bench.rs`, `main.rs`, `theme.rs` | **124** |
| composer state (`wb_compose`) inside `pane/bench.rs` | 29 |
| `bench_deliver` — bytes into the pty | 9 |
| `pane_id` | 4 |
| focus handle | 4 |
| `self.mode` (all forms) | 12 |

The tenancy pane's writeup reports the pane's `mode` at 39; measured here it is
12 `self.mode*` references, 19 occurrences of the bare word, and 18 of
`is_agent()`/`agent_now` combined. No reading gives 39. It does not change that
writeup's conclusion — the `bench_deliver` argument stands on its own — but the
drawing category is smaller than 39 makes it look, and the number is corrected
here rather than propagated.

## Where this came from

Parker annotated `reports/2026-09-21-workbench-text-entry.html` — a brief that
offered three placements of the boundary between our editor and the agent's —
and rejected all three in favour of a fourth. His note on the recommended card:

> BEFORE WE PROCEED — there is a CRITICAL decision we are making to DECOUPLE
> workbench from the terminal surface — workbench will operate on an API AGAINST
> THE TERMINAL PROCESS SEPARATE from the terminal view… This API will have a
> SPECIFIED AND CONSTRAINED AND WELL DEFINED SHAPE! It will put when it needs to
> start a new agent, CONSUME the workbench JSON when the agent responds via agent
> 2 UI etc… BIG HARD DECISION… but let's start it now and rip off the bandaid!

And in chat, the same day:

> WE WILL DECOUPLE WORKBENCH FROM THE TERMINAL! — painful, but let's rip the
> bandaid off now… Tie off any work in progress, but ultimately … yea don't step
> over our bounds because a re-work will be ultimately necessary.

So: **no more fixes to the mirror.** The fifteen small repairs the brief costed
out under option A are not being made; they would be thrown away.

## His seven notes, in full, so nothing lives only in chat

Anchors are element ids in `~/Downloads/2026-09-21-workbench-text-entry.html`.

| Anchor | Note |
|---|---|
| `fig-02-the-mirror-in` | "This begs the question of SHOULD WE GO FULL ASYNC from the terminal when we are in workbench mode…" |
| `row-ctrl-c` | "SUPPPPPER IMPORTANT — if someone THINKS this is a text editor because it looks like a text editory and then then TD thinks it is a terminal because it is a mirror… the person will SHUT DOWN THEIR SESSION ACCIDENTALLy — I ahve had this painpoint in the past!" |
| `row-shift-enter` | "ouch - pain" |
| `row-accessibility` | "I like this a bunch - and here is why…. one day we will have agnets doing phone calls and having a screen reader will be super for that --- do we do it now early before that problem is defined? NOPE!" |
| `fig-04-what-1-000` | "CLick drag to expand is neat, but it should definitely also AUTO expand if someone is dumping a PILE of text in…." |
| `fig-05-where-a-keystroke` | "Interesting!" / "This archtiectural is simple and clear, but is it correct?" / "Again the question from before around async… if we fully async the workbench from the terminal and join them by a sort of API… that is a substantial rebuild. I sincerely thought that was what we were doing on the initial build… BUT the result of headless from the terminal PROCESS, but coupled with the TD-TERMINAL VIEW of that process felt right at the time.. more and more we feel like we will decouple …" |
| `verdict-a-widen-the-mirror` | the decision, quoted above in full |

## Notes for a fresh session

- **This is not the client-server split.** That one (`docs/plans/client-server/`,
  shipped and flipped 2026-09-10) separates the *window* from the *PTY host*, so
  terminals outlive the window watching them. This one separates the *workbench*
  from the *terminal view* of the same process. Both can be true at once and the
  new API rides on the host that split already built.
- **The measurements behind the problem statement** are in the brief and are
  reproducible without building the app: the pure decision functions
  (`Line`, `Edit`, `line_edit`, `follows`, `composer_pt`, `composer_hidden`) lift
  out of `workbench.rs` by line range and compile standalone. The method is in
  the brief's "Reproduce every measurement" modal.
- **The tree was not clean when this plan was opened.** 444 uncommitted lines
  across `surface.rs`, `mcp.rs`, `workbench.rs`, `benchdraw.rs` and the TDSP spec
  belong to another session doing the TDSP 0.4 `layman` work. Left untouched.
