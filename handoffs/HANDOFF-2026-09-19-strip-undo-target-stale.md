# Handoff — the tab strip's undo pointed at the old default (2026-09-19)

## Status

Landed on `fix/the-second-click-backs-out-to-your-branch`, two commits, pushed.
**Pull request 596, open, awaiting review.**
`66776b3` the fix · `a6b04e6` the guard.

**Not installed.** The running window (`td-569f452-allin`) keeps the old
behaviour until it is rebuilt and restarted. **Workaround with no rebuild: one
press on the `ALL` chip at the top of the left bar.**

## What's done

**The fix.** `Scope::toggled` (`app/src/tree.rs`) returned `Scope::All` to mean
"undo this pin". Correct when written on 2026-09-10, because `All` *was* the
resting scope then. PR 546 moved the resting scope to `Branch` on 2026-09-18 and
left this pointing at the far end of the bar — so the change made to stop the
strip carrying every tab shipped a one-click gesture that puts every tab back.
It now returns `Scope::default()`.

Two call sites improved for free: the **UNFILED** heading passes `Scope::All` by
name, so before this it answered `All` whichever way it was pressed (widen, no
way back); and the **scope chip** carried a hand-rolled duplicate of the same two
lines, which had already disagreed with `toggled` for a day. All three controls
now ask the one function.

**Three tests**, all verified red before the fix:

| test | what it holds |
|---|---|
| `scoping_to_the_branch_you_are_already_on_backs_out_to_the_branch_you_are_in` | the unit contract, both UNFILED directions included |
| `clicking_the_group_you_are_already_in_twice_cannot_bomb_the_strip` | the gesture end to end, over the real 31-tab window shape |
| `nothing_names_the_resting_scope_by_value_where_it_means_the_default` | the source guard: `toggled` may name no `Scope` variant by value |

The guard was proven by mutation, both legs:

| mutation of `toggled` | behavioural tests | the guard |
|---|---|---|
| `Scope::All` (the original bug) | fail | fail |
| `Scope::Branch` (identical today, stale tomorrow) | **pass** | **fail** |
| `Scope::default()` (shipped) | pass | pass |

The middle row is the whole reason the guard exists — a behavioural test
structurally cannot catch a right value going stale.

## How to run/verify

```
cd /home/parker/Work/terminal-delight/app && cargo test --locked --bin terminal-delight tree::tests
```
```
cd /home/parker/Work/terminal-delight/app && cargo clippy --locked -- -D warnings && cargo fmt -- --check
```

Last run: 1312 passed / 0 failed; clippy clean; fmt clean.

To see it in the window, build and install a `td-<sha>-<label>` and repoint
`~/.local/bin/terminal-delight`, then restart the window — a merge alone changes
nothing Parker sees.

## Not done / next

- **PR 596 needs review and merge.**
- **Issue 603** (`follow-up` label) + APES ticket
  `make-the-scope-chip-distinguish-a-pin-from-the-branch-default-mu822inw`: the
  chip reads identically for a pin and for the branch default. That is the
  invisibility this bug rode in on. Harmless on the second press now, but a
  pinned strip still widens to the whole project the first time you activate a
  sibling group — on Parker's window, 4 tabs to 14 with no gesture asking for it.

## Watch out

- **Shared worktree.** `/home/parker/Work/terminal-delight` was moved off
  `bench/defaults-and-dials` (which was fully merged into `origin/main`, nothing
  lost) onto the fix branch. Twenty-two other worktrees are live under
  `~/Work/td-*`; another session may switch this one again.
- **The scope is deliberately not persisted** since PR 546, so you cannot read a
  window's scope from `~/.config/terminal-delight/sessions/<n>.toml` to diagnose
  this class of report. That absence is also the cheapest proof of a binary's
  vintage: a `scope` key in that file means a pre-546 build.
- **`lean-ctx` refused every call** for the first third of this session
  (`agent capacity reached: 15/15 live workers`). Fall back natively and say so.

## Where it's recorded

- APES episode: `episodes/2026-09-19-the-undo-pointed-at-the-old-default.md`
- APES tickets: the fix ticket closed with deliverable; follow-up ticket open
- lean-ctx: `conventions/default-move-strands-value-named-callers`,
  `testing/guard-needs-a-mutation-the-behavioural-tests-pass`,
  `testing/prove-the-build-from-its-output`, plus the session decision
- file-memory: `moving-a-default-strands-whoever-named-the-old-one.md`,
  `a-guard-earns-its-place-by-a-mutation-that-passes.md`
- Session harvest: `handoffs/2026-09-19-strip-undo-target-stale.cdx`
