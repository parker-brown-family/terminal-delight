# Status: The Workbench

**Difficulty: 8/10** — a new pane face, a new wire protocol other people's agents
will target, and a launcher that starts processes. The protocol is the part that
is expensive to be wrong about: a catalogue, once agents are told it, is a promise.
The UI is cheap to unwind; the vocabulary is not.

**Built overnight 2026-09-16/17, unattended.** Parker was AFK with no approval
channel, so the gates were collapsed into this file and the decision brief that
ships with it. Every choice that would normally have been a gate question is
written down as a decision with its alternatives, in `docs/spec/`, and marked in
the brief's grill for him to overrule.

## Progress

- [x] Clean room: `~/Work/td-workbench`, branch `workbench/surface-protocol`, cut from `origin/main` at eb84c1d
- [x] `app/src/surface.rs` — TDSP v0.1: envelope, six kinds, weights, validation, unclassified fallback
- [x] `docs/spec/td-surface-protocol.md` — the API documentation
- [x] `app/src/workbench.rs` — per-pane state: face toggle, rail tabs, selection, history
- [x] Renderers for the six kinds (pure layout model + gpui drawing)
- [x] Pane integration: header toggle, workbench body, in-pane right rail
- [x] Transport in: watched directory + MCP `present_surface`
- [x] Action channel out: journal file + PTY injection
- [x] Launcher: project/model/effort picker → recipe → host spawn, with the protocol prompt
- [x] Tests green, fmt + clippy clean
- [x] Decision brief
- [x] Retention: dead sessions aged out whole, a per-pane disk cap, the live
      session and the action journal exempt (2026-09-17)

## Surprises & discoveries

- The host already types a recipe line into a freshly spawned pane
  (`host.rs` `spawn_pane` → `Msg::Input(format!("{recipe}\n"))`). The launcher is
  therefore a recipe builder, not a new process-management subsystem. This is the
  single biggest saving in the whole plan.
- `make_pane` in `main.rs` is documented as "the only way a pane is made after the
  window is up" and takes a `PaneRestore { cwd, resume, .. }`. Launching an agent
  with a chosen model is one struct away from an existing funnel.
- The terminal is already a bidirectional channel to the agent. The action channel
  back from the Workbench does not need inventing: TD types it into the PTY, which
  is exactly how a human answers an agent today.
- `mcp_tail.rs` already reads each agent's JSONL transcript. A fenced `td` block in
  an agent's own output is therefore a third writer that needs no cooperation
  beyond printing, which is what makes this work for Codex and Gemini too.

- The retention rule was scored as a per-pane file cap and measurement moved it.
  On the day the store was first looked at, nine session directories existed and
  eight were one-shot demo and screenshot keys holding six files each — the
  growth is whole dead SESSIONS, not files piling up inside a live pane. The cap
  stayed, at 512 rather than the shelf's 64, because `PANE_HISTORY_CAP`'s own
  comment promises the disk is the archive and an equal cap would make that a
  lie; the rule that actually reclaims anything is the thirty-day one.

## Decisions

1. **Semantic kinds, not components.** The agent says `architecture`; TD owns what
   architecture looks like. A2UI's fixed-catalogue finding, taken whole.
2. **A bounded catalogue of six.** artifact, markdown, table, architecture,
   changeset, decision. Everything else renders `unclassified` and says so.
3. **Three writers, one format.** MCP verb (validated), file drop (any language),
   transcript fence (zero cooperation). No escape sequences, no stdout scraping.
4. **Unknown is a variant everywhere.** An unparseable surface is kept and drawn
   as unclassified with its reason; a missing field prints `unavailable`.
5. **The Workbench is a face, not a pane type.** Every pane has both faces; the
   toggle is per-pane and persisted. A shell pane's Workbench is empty and says so.

**Turned out to be: 8/10 was right, for the reason predicted.** The UI took an
evening and could be unwound in one; the protocol took the thinking. The part
that surprised me was the launcher, scored as a day's work and delivered in
twenty lines because the host already types a recipe into a fresh pane.

## Verified

- `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, full suite:
  **1,058 tests green**, 83 of them new across the five new modules.
- **Photographed on the real build**, 2026-09-17: six demo surfaces written as
  ordinary files by one process appeared on a pane's bench in another, the
  header drew the unseen count, and the decision rendered with its recommended
  option lit. The photograph found three defects 1,050 tests had not — a
  duplicated reason line, a heading that clipped instead of wrapping, and a
  rail that defaulted to ticks. All three fixed.

## NOT verified

*(As written the morning of 2026-09-17; superseded by the day's live rounds,
kept for the record.)* **No mouse gesture in this branch has ever been
pressed.** This shell has a virtual keyboard (`wtype`) and no virtual pointer,
so the toggle, the rail rows, the action chips and the launcher are compiled,
rendered and unclicked. Parker pressed the header toggle once on the first demo
window and it worked; everything else is his to try.

`alt+w` did not fire when injected through `wtype`, which is unexplained — it
may be the injection rather than the chord, and it is the first thing to check
by hand.

*Since then:* Parker pressed the whole surface through a day of live rounds —
the toggle, the rail, the composer, the question flow, the review gallery.
`wtype` is never used at a window again (it types into whatever has focus;
`ctl bench type` replaced it). One click under the warp has been read back
from the log, at the centre; the corners are tested numerically and still
unpressed by a hand.

## Next

- Five minutes of clicking, by a person. That is the whole outstanding test.
- Slice 0 equivalent: count how many surfaces a real session produces before
  widening the catalogue past six.
- Held panes and restored panes: a surface is currently per-pane and per-session.
- The machine-global `AGENTS.md` paragraph (`docs/spec/agents-md-snippet.md`),
  after the build is installed and used for a day.

## The architecture pass — 2026-09-17, afternoon

Parker's reading after a day of live rounds: *the front end is functionally
there; the backend is spaghetti and needs a refactor plan and pass.* The plan is
`01-architecture-pass.md`; this is the state.

- [x] `screenread.rs` — every function that reads the terminal grid as text, in
      one module, under a law: each public reader has a test transcribed from a
      real screen, enforced by a source scan. The law caught five untested
      readers on its first run. (`cd8005c`)
- [x] `pane/bench.rs` — the bench's 22 methods and key handler out of `pane.rs`
      (11,368 → 9,487 lines) as a CHILD module, so no field changed visibility.
      A scan in pane's tests fails if a `fn bench_` grows back; mutation-tested.
      (`ae38d42`)
- [x] TDSP 0.2, with a test that parses 0.1/0.2/0.9 and refuses 1.0. The bump
      had been missed through four field additions. (`03a1358`)
- [x] The composer diagnostic's four findings dispositioned in the plan; its
      first finding root-caused to the mirror replacing where the PTY appended.
      (`72950ad`)
- [x] `body()` — one kind-match per embodiment (slice 3): **disproved.** The
      kind matches are already exhaustive, so a new kind fails to compile
      until every embodiment draws it. Nothing to collapse. (#486)
- [x] Mechanical guard for "a renderer contains no decisions" — a scan of
      the renderer's code for a threshold or an environment/clock read,
      mutation-tested three plants caught, two allowed shapes passing.
      (`47cbc76`)
- [x] The warp on the bench — Parker decided, built, hit-tested through the
      inverse. (`46ec6b6`) Then the residue closed: the wheel and the pointer
      go through the same inverse and the corners are tested. (5299c1d)
- [x] The bench in the vignette: exempt. (`5b3f1d6`)
- [x] `ctl bench` verbs reach the focused pane first. (`2d63f9a`)
- [x] `host_socket` root-caused — the tests' wait, not the host. (`9722402`)
- [x] The spec's map names `screenread.rs` and the debugging flags. (`120ab9c`)
- [ ] The mirror's future — a design pin, #490

**Surprises.** A test written as `for i in 0..SETTLE_SWEEPS` passed with the
constant set to zero — vacuous under the exact mutation it existed to catch.
A gate written as `cargo clippy | grep error; echo OK` committed a clippy
failure, because grep succeeds when it FINDS errors; every gate now reads the
tool's own exit code. And `host_socket`'s PTY-timing test flaked four times
under a parallel release build and passed every time alone — an issue when the
branch lands, not a memory.

**Difficulty in hindsight: 6/10 for the pass, as scored.** The two big cuts
were mechanical and the compiler and 1,172 tests caught every mistake the mover
made — including two of its own. What was NOT cheap was the day before it: nine
of the day's defects were rules written where no assertion could reach them,
which is the thing the pass exists to end.

## The second pass — 2026-09-17, evening

Parker: *"can you fix everything?"* Six follow-ups had been filed at the
tie-off; five are fixed and one is a design question. Each was falsified
before it was fixed, and two did not survive it whole:

- **The warp's residue was not what its commit said.** "`ScrollHandle` has no
  public setter" was false, and "the displacement at the composer is small"
  was false too: measured at the live curvature, a corner control is shown
  76.8 px from where it was laid and the composer's corner 62.5 px. The
  wheel and the pointer now go through the inverse, and the corners are a
  test. (#485)
- **The flake was in the tests.** Reproduced one run in eight under load; the
  wait returned on the terminal's echo of a line before `cat`'s copy of it,
  which then arrived live after a snapshot that held the echo. The host's
  fence was keeping its promise. (#438)
- **Slice 3 was already true.** The renderer's kind matches are exhaustive;
  only the guard was missing. (#486)

**Verified:** fmt, clippy `-D warnings`, the full release suite, on their own
exit codes, before each of the seven commits. **Not verified by a hand:** a
corner click and a wheel over the bent edge of a long draft under
`TD_HITDEBUG=1` — the numbers are tested, the gesture is not.