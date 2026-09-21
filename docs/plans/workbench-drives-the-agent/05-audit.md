# Audit: the decouple, against the scorecard that asked for it

**Branch:** `origin/plan/workbench-decouple-async` @ `e022f42` — 10 commits ahead
of `origin/main`, 0 behind.
**Audited:** 2026-09-21, from `~/Work/td-decouple` and via the shared object
store. Everything below was run or read, not taken on report.

**Verdict: the channel is sound and should merge. The metric it was built
against is not met — 11 of 23, where Gate 1 asked for 20 — and the shortfall is
one coherent piece: the composer still has no selection.**

---

## The gates, run rather than assumed

| Gate | Result |
|---|---|
| `cargo fmt --check` | clean |
| `cargo clippy --locked -- -D warnings` — the invocation CI runs, no `--all-targets` | clean, exit 0 |
| `cargo test --locked` | exit 0 |
| `git merge-tree --write-tree origin/main <branch>` | **clean**, tree `55fe57e` — no conflicts |
| installed `~/.local/bin/td-agent-hooks` vs branch source | **byte-identical** |

## What landed, and it is good work

- **The channel exists and is specified.** `docs/spec/td-agent-channel.md`, with
  inbound types (`prompt`, `question`, `waiting`, `released`, `answered`,
  `reply`, `notify`) and outbound verbs.
- **The verbs are named for situations, not capabilities** — `interrupt` (one
  `0x03`), `end` (two, so the alternate screen comes down), `answer`, `say`,
  `launch` — with `keys` retained as *the* one impure verb and **always
  journaled**. That is the answer to Gate 1's objection that "the one impure
  verb is the one everyone will reach for", and it is better than the objection.
- **Three of the three measured divergences are closed at the table.**
  `line_edit` now takes `shift`; `Edit::Up`, `Edit::Down` and `Edit::Newline`
  exist. `shift+enter` is a line break instead of `Edit::Submit`, which was the
  keystroke that emptied the box (#614).
- **Undo is real** — `Edit::Undo` and `undo: Vec<(String, usize)>` on the line.
  A document can have one; a mirror could not.
- **Ctrl+C is freed.** `bench_interrupt`'s own comment: *"One `0x03`, recorded
  first. Never the copy chord."* The accidental-session-kill (#539) is answered
  by design rather than by a stopgap.
- **The hook's `no-bench` guard is correct**, which is the thing that could have
  hurt every agent on the machine rather than just the bench. `fresh()` requires
  `bench.json` to exist, name the workbench face, and carry an `at_ms` within
  ±4s. Not fresh → `released no-bench` → immediate `exit 0`. Inside the wait it
  re-checks at 4 Hz and exits `stale` if the bench stops heartbeating or the
  face flips. **An agent with no bench open never waits.**

## The scorecard, re-measured

Gate 1's metric: *from 4 of 23 to at least 20 of 23.* Measured against the
branch:

| | Gesture | Before | Now |
|---|---|---|---|
| ✅ | ↑ ↓ | ✗ | **✓** `Edit::Up`/`Down` |
| ✅ | shift+enter | ✗ | **✓** `Edit::Newline` |
| ✅ | ctrl+z undo | ✗ | **✓** |
| ✅ | ctrl+c copy | ✗ | **✓** freed; interrupt is its own verb |
| ✅ | ← → · ctrl+← → · home/end · ctrl+a · click · ctrl+v | ✓/◑ | ✓ |
| ❌ | **shift + any motion** | ✗ | **✗ still** |
| ❌ | **click and drag** | ✗ | **✗ still** |
| ❌ | **double / triple click** | ✗ | **✗ still** |
| ❌ | **shift+click** | ✗ | **✗ still** |
| ❌ | **ctrl+x cut a selection** | ✗ | **✗ still** — nothing to cut |
| ❌ | pageup / pagedown | ✗ | ✗ — no arm in `line_edit` |
| ❌ | type-size ladder (#616) | ✗ | ✗ — constants unchanged |
| ❌ | IME / accessibility | ✗ | ✗ — out of scope by decision |

**11 of 23.** The five bold rows are one missing piece, not five.

### The one thing that did not land

`Line` still carries `marked: bool`. There is no anchor and no range, so:

- `line_edit` consults `shift` **only for `enter`** — there are no shift+arrow
  arms at all.
- `Hit::Composer` is **not** in the drag-start set (`Hit::Arm | Hit::OpenRow(_)
  | None`), so a drag over the draft does nothing.
- The field's own doc comment still reads *"there is no mouse drag over the
  draft and no shift+arrow here"*, which is accurate and honest.

A `Selection { anchor: Caret, head: Caret }` **does** now exist on the branch,
with correct backwards-drag handling — but it is for dragging over the bench's
**reading** surfaces (PR #621, `bench/text-selection`), not over the composer.
Two selection models, one of them not wired to the box this feature is about.

This is the half of Parker's original ask that is still open. His words were
*"with highlighting, with mouse highlighting, with copy and paste"*.

## Also still open

- **#617** — `pane.rs:5968` still gates the bench mouse-down on `MouseButton::
  Left`, so a right-click still opens the *terminal's* copy tray over the bench.
- **#616** — `COMPOSER_STEPS` is still `[8.5, 7.25, 6.25]` against a `9.5` floor.
  The ladder is still inert at the default gauge.
- **#615** is likely closed by construction (no second copy to diverge) but was
  not re-measured; check before closing it.
- All six issues (#614–#619) are open.

## One finding outside the code

**Two `PreToolUse` hooks on `AskUserQuestion` are live in
`~/.claude/settings.json`.** One is `~/.local/bin/td-agent-hooks` at
`timeout: 600` — correct, installed by `scripts/install-agent-hooks.sh`, which
backs the file up and merges rather than clobbering.

The other is ad-hoc inline `jq` at `timeout: 5`, appending to
`~/.claude/askhook.log` (currently 4 lines) with its own 5 MB rotation. It is
**not in the repository**, it is experiment debris from settling the
hookability question, and nobody owns it. Recommend removing it — it costs a
`jq` spawn on every question round and it will outlive everyone's memory of why
it is there.

## Recommendation

**Merge it.** It is green on the gates CI runs, it conflicts with nothing, it
strictly improves on main, and the architecture is right.

**Then take selection as its own slice**, because that is what the metric is
short by and it is the half a person notices. It is now a much smaller job than
it was this morning: the composer owns its draft, `Selection { anchor, head }`
already exists in the same crate with the hard part solved, and the only reason
it cannot be pointed at the composer today is that `Line` holds a boolean where
it needs two carets.

**Do not close #614–#619 on the merge.** #614 and #539 are answered; the rest
are not, and #616 and #617 are untouched.
