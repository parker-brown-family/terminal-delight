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

**No mouse gesture in this branch has ever been pressed.** This shell has a
virtual keyboard (`wtype`) and no virtual pointer, so the toggle, the rail
rows, the action chips and the launcher are compiled, rendered and unclicked.
Parker pressed the header toggle once on the first demo window and it worked;
everything else is his to try.

`alt+w` did not fire when injected through `wtype`, which is unexplained — it
may be the injection rather than the chord, and it is the first thing to check
by hand.

## Next

- Five minutes of clicking, by a person. That is the whole outstanding test.
- Slice 0 equivalent: count how many surfaces a real session produces before
  widening the catalogue past six.
- Held panes and restored panes: a surface is currently per-pane and per-session.
- The machine-global `AGENTS.md` paragraph (`docs/spec/agents-md-snippet.md`),
  after the build is installed and used for a day.
