# Status: The workbench drives the agent

**Difficulty: 9/10** — a new API boundary between the workbench and the agent
process, replacing a coupling that four existing subsystems assume. It is a
protocol, so being wrong is expensive for a long time and other things start
targeting it; and the failure it fixes is silent, which means today's tests say
nothing about whether the new one is correct. → **four gates.**

**Turned out to be:** a 9 for the *shape* — a protocol other things will
target, and one silent failure the tests could not see (the marker as a
surface) — and a 6 for the *ceremony*: the gates were written and not
approved, because Parker was AFK and asked for it built, and the two things
that actually needed him (the third cost, the interrupt chord) are one grill
each in the brief. Built in one session on 2026-09-21: the gate green, PR
#624 open, the release installed beside the current one as
`td-00860a5-decouple` with the symlink untouched, the hooks wired.

- Gate 1 — Product: **APPROVED 2026-09-21** — Parker, in chat: *"Great - love it
  yes."* Approved after the amendment below, which was folded in before the
  approval rather than after it.
- Gate 2 — Architecture: **written 2026-09-21 by the DECOUPLE-ASYNC pane**
  (`02-architecture.md`, with the channel's contract in
  `docs/spec/td-agent-channel.md`). Parker, before going AFK: *"you go ahead
  and get this implemented as well — any significant decisions you make can
  fall into a doc for me to read after — GO FULL SEND and I am fully AFK so you
  cannot ask me anything!"* So the gate was not approved in chat; the decisions
  it makes are listed below under **Decisions made while you were away** and
  drawn in the brief, for him to overrule.
- Gate 3 — Program Design: **written 2026-09-21**, same pane, same terms
  (`03-program-design.md`).
- Gate 4 — Slice plan: **written and built through slice 3 on 2026-09-21**
  (`04-slices.md`). What was built, what was held, and what the next pane
  picks up are in that file.

The earlier note stands as history: Parker said *"the async work will belong
to another agent. Let's wrap up our WIP BEFORE we start the ASYSNC"*, and the
pane that wrote Gate 1 stopped there. This is that other agent.

## Decisions made while you were away (2026-09-21)

Each of these is drawn and annotatable in
`reports/2026-09-21-decouple-async-the-agent-channel.html`. The number is the
order to read them in; the first is the one to overrule if any.

1. **"The third cost" is answered with option 3, and the one impure verb is
   kept and journaled.** A question travels to the bench through a hook,
   whole, before the picker paints; the answer travels back as a file the
   hook is waiting for, and the picker never paints. **Measured, not
   inferred:** driven under a pseudoterminal against Claude Code 2.1.274 on
   this machine, a `PreToolUse` hook returning `updatedInput` with `answers`
   skipped the picker and the model received the answer (the LAST option,
   chosen deliberately so a default-first pick would have shown). Where the
   hook is not waiting — no bench open, an older window, a harness with no
   hooks — the answer goes as keys through the same one gate it always did,
   and every such write is journaled as `keys` so the fallback is visible.
2. **The composer is a document, not a mirror.** Nothing leaves it until send.
   Send is ONE write: a bracketed paste plus a return. Ctrl+C copies; the
   interrupt is the strip's stop control and `ctrl+g`; shift+enter is a line;
   up/down walk the draft; undo is `ctrl+z`.
3. **The two features the mirror gave away are answered:** history is the
   bench's own sent log (up on an empty draft); slash commands still run
   (Claude Code parses `/model x` at submit, pasted or typed); completion is
   not offered and the composer does not pretend to — the terminal face has it.
4. **The channel is files, not a wire, and it needs no host upgrade.** Two
   journals, one directory the agent already knows, a marker the hook reads.
   The host is untouched, on purpose: it only upgrades by dying.
5. **The bench stays inside the pane for now.** The 124-reference ownership
   move is the next slice, deliberately after the view coupling is gone, and
   after the tenancy pane's conversation key has landed — two agents on
   `Bench` at once is how a merge eats a day.
6. **Hooks are installed into `~/.claude/settings.json` by a script, and
   every one of them exits 0 on any error.** The blocking wait on a question
   happens ONLY while this window says a bench is open on that pane and said
   so within four seconds; otherwise the hook returns at once and the picker
   paints exactly as today. `scripts/install-agent-hooks.sh --uninstall` takes
   it all back out.

## The afternoon: merged, demonstrated, hardened (2026-09-21)

- **#624 merged** into main at `be60bf2` on Parker's instruction, after CI.
- **Demonstrated live** in a hosted window on the new build (`TD_SESSION=decouple-demo`,
  Parker's own opus agent in pane 3): round one fell back to keys when the
  face changed mid-wait and was journaled as such; round two took the file
  road (`You chose Autumn`, no picker); round three measured the multi-select
  join (`ANSWERS="Red, Blue"`). The demo window and its host were left running
  for him.
- **Hardened** as TDAC 0.2 in PR #635 (`72408ea`): the release reasons, the
  marker schedule, answered-before-asked, the cap, reply ids, recall edits,
  `session`/`pane` on outbound lines, journal rotation, `ctl bench choose`
  reaching a channel card's Submit. Twenty channel tests including the real
  adapter driven end to end; twelve adapter tests. Installed beside the others
  as `td-72408ea-decouple`; the symlink still points at `td-7d41cb5-main`.
- **The retro** is `05-hindsight-burndown.md`; the week's APES burndown is
  `CEREMONIES/burndowns/2026-09-21.md` in the registered tree. Follow-ups
  #628–#633. The round navigator is #634, by the pane that filed #619, on top
  of this work.

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

The tenancy pane's writeup first reported the pane's `mode` at 39; measured here
it is 12 `self.mode*` references. **Settled and corrected at their source**
2026-09-21: their figure came from `git grep -c mode`, which counts lines
containing the substring — catching `model` twelve times, `Dial::Model` four and
`wb_model` three. `self.mode*` is exactly 12 on both readings. Their conclusion
was never affected; the `bench_deliver` argument stands on its own.

### Line numbers — read these on origin/main, not on this branch

**This branch is 66 commits behind `origin/main`.** Every finding below was
re-verified against main on 2026-09-21 and all of them still hold, but the line
numbers in the four captured issues (#614, #615, #616, #617) and in the comment
on #539 were read from this branch. Whoever picks up Gate 2 will be on main.
These are main's:

| What | On origin/main |
|---|---|
| `line_edit` — still `(key, ctrl, alt)`, no shift, `"enter" => Edit::Submit` | `workbench.rs:1625` |
| `Line::seek` — still does not clear the mark | `workbench.rs:662` |
| `Line::apply` — the one path that does clear it | `workbench.rs:685` |
| `Line::mark_all` — ctrl+a, shipped 2026-09-18 | `workbench.rs:562` |
| `composer_pt` / `composer_hidden` | `workbench.rs:2228` / `2263` |
| `COMPOSER_STEPS` / `COMPOSER_MIN_PT` / `COMPOSER_SHARE` — unchanged | `workbench.rs:2251` / `2256` / `2275` |
| `bench_click` → `line.seek(to)` | `pane/bench.rs:1468` → `1494` |
| `bench_deliver` → `write_through` (this branch inlines the notifier instead) | `pane/bench.rs:1832` → `1843` |
| `bench_end_agent`'s two interrupts | `pane/bench.rs:734` |
| `Dispatch::Keys` — declaration and handler | `workbench.rs:2488`, `pane/bench.rs:2239` |
| `keystroke_bytes` | `pane.rs:6939` |
| the bench's left-button-only mouse guard | `pane.rs:5888` |
| the right-click that opens the terminal's tray | `pane.rs:5942` |
| `bench_key` called, then copy / find / cut below it | `pane.rs:4732`, then `4866` / `4910` / `4926` |

**One thing main has that this branch does not:** `line_edit` is called from
**two** sites in `pane/bench.rs` (`509` and `2012`), not one. Check both before
concluding anything about which keystrokes reach the table — the second is
likely the reading-mode arrow work filed as #611.

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
