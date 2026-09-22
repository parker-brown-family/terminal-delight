# Handoff — pause and resume a turn from the bench strip (2026-09-22)

## Status

**Landed.** Merged by Parker as [#654](https://github.com/parker-brown-family/terminal-delight/pull/654)
(`5e5eebb`). All four of my commits are ancestors of `origin/main`; the local
branch name `bench/pause-and-resume-the-turn` has since been deleted, so find the
work by SHA (`9499efe`, `4d2c466`, `55f227d`, `5c295c8`) rather than by branch.
Nothing uncommitted. Nothing of mine is pending review.

## What's done

- **`PAUSE TURN` / `RESUME TURN` on the bench strip**, ahead of the two dials.
  One `0x03` stops the turn and leaves the session alive; the resume sends
  `RESUME_SAY` = `"Proceed with the turn."` down the ordinary message path.
  *Verified:* four table tests in `workbench.rs`, plus five mutations each
  reverted and each caught by the test that guards it.
- **`AgentState::Paused`** — the only rung of `agent_state` not read off a
  screen. Sits at the top of the ladder because it must beat `reading` (the
  interrupt stamps that clock), `done` (a bell fires on the way back to the
  prompt) and `asking` (a stale picker outlives the turn). Guarded by
  `!thinking` and `!exited` so a real sensor can overrule it.
  *Verified:* a seven-row table test naming each rung it clears.
- **`dials_live(Paused) == true`** — the whole point of stopping a turn.
- **The `AGENT` label removed** from `title_card`.
- **`draws_waiting_block`** — a question opened from the rail is no longer
  pinned a second time under its own card; the card's round navigator became
  pressable to pay for the pin's loss.
- **A peer-found defect fixed** (`26b20d4`, theirs): `wb_paused_ms` was not
  cleared when an AGENT departs, so a freshly launched agent was born `Paused`.
  Their source gate now names fifteen fields; I added the four the transcript
  store brought in parallel.
- **A 120ms rising-edge clear** (`55f227d`) beside the once-a-second sweep. Both
  are kept: the fast scan is gated on `scroll_settled` and does not run while the
  scrollback is being walked.

## How to run / verify

```bash
cd /home/parker/Work/terminal-delight/app && cargo test --locked --bin terminal-delight workbench::tests
```
```bash
cd /home/parker/Work/terminal-delight/app && cargo clippy --locked -- -D warnings
```

**Reconcile the count or the number means nothing** — a shared `app/target`
serves stale binaries here:

```bash
cd /home/parker/Work/terminal-delight/app && cargo test --locked --bin terminal-delight -- --list | grep -c ': test$'
```

Listed must equal passed + ignored. On `2d46e72`: 1579 = 1573 + 6, all five of
this feature's tests named in the binary.

## Not done / next

- **Nobody has seen the control rendered.** Issue
  [#695](https://github.com/parker-brown-family/terminal-delight/issues/695)
  carries the falsifiable width question *and its invalidation criterion* — run
  that first; the arithmetic says it is probably fine and the issue closes
  `invalid`.
- **A pause does not survive a window restart.** Deliberate; Parker's call to
  leave it.
- **Tooling idea:** a `ctl bench pause` verb. `ctrl+G` reaches `bench_pause`
  only while the composer is armed, which is why the feature cannot be driven
  headlessly today.

## Watch out

- **`cargo test` can answer from cache; `-- --list` has to build.** Both were run
  minutes apart here and disagreed — green first, nine compile errors second.
  The errors belonged to a sibling's uncommitted work, not to main. Attribute
  against the commit (`git show HEAD:<file>`), never the checkout.
- **This worktree is shared.** `git status` shows every agent's dirt with no
  authorship. Never `git add -A`; diff each hunk before committing.
- **Do not try to place a TD window by script.** `hl.dsp.focus` takes no
  `workspace`, there is no view-workspace dispatcher, and the classic
  `hyprctl dispatch workspace N` is inert. Probing `hl.dsp.window.*` acts on
  whatever is FOCUSED — it floated Parker's own pane here. See
  `scripts/workbench-smoke.sh` for the house rig.
- **`instance::tests::one_window_per_key_and_the_lock_dies_with_it` flakes**
  under full-suite parallelism — known, filed as #579 and #607, passes alone.

## Where it's recorded

- APES episode: `projects/terminal-delight/episodes/2026-09-22-pause-and-resume-the-turn.md`
- Session package: `handoffs/2026-09-22-pause-and-resume-the-turn.cdx`
- Decision brief (the state + decision tree, drawn): `reports/2026-09-21-pausing-a-turn.html`
- lean-ctx: `ctx_session` decision recorded 2026-09-22
- file-memory: `a-generic-check-before-a-specific-one-swallows-every-case`,
  and updates to `a-peer-agents-message-is-a-claim-not-a-fact`,
  `a-shared-target-dir-serves-a-stale-test-binary`,
  `dont-probe-hyprland-on-a-live-session`
