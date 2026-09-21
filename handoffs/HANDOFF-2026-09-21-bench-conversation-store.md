# Handoff — the bench belongs to the conversation, store half (2026-09-21)

## Status

**Landed.** Merged to `main` as `7f20863` (PR #627), verified afterwards rather
than assumed: `git merge-base --is-ancestor 4efd02a origin/main` says yes, and
`app/src/benchstore.rs` is 1130 lines in main's tree.

**The worktree and branch are gone.** `~/Work/td-bench-store` and
`origin/bench/conversation-store` were reaped by another session after the merge.
Nothing is lost — the work is on main — but do not go looking for them.

Slices 1–2 of 6. Slices 3–6 are specified and unbuilt.

## What's done

| Slice | What landed | How it was verified |
|---|---|---|
| 1 | `Bench::clear_surfaces`, called from `set_mode`'s departed edge. Drops surfaces and everything keyed by a `SurfaceId`; keeps the face, shelf and rail preference. | 3 tests. Mutated twice in opposite directions — under-clearing fails on the stale selection, `*self = Bench::default()` fails on the reader's face. |
| 1b | The function destructures `Bench` exhaustively, so a future field is a compile error until somebody decides whether it survives a clear. | Proved by adding a field: `error[E0027]: pattern does not mention field 'pinned'` at that line, then reverted. |
| 2 | `benchstore` — `conversations/<root>/<seq>/` with `surfaces/` and an append-only `turns.jsonl`. Pure std, no gpui. | 20 tests. Mutated three ways: load-current-segment-only, lexical sort, blacklist instead of whitelist — each caught by the test that encodes its lesson. |
| 2b | `terminal-delight conversation <root>` — the headless read-back verb, plus `--json`. | Driven against a store on disk, not only compiled. Registered in main's `VERBS` table as `Piping::Streams`, which required a real case in `app/tests/broken_pipe.rs`. |

Nine defects found by `/code-review` at high effort **before** the merge and
fixed in it. The two largest: `load` ordered by filename stem while promising
*oldest first* (and both tests covering it used the one id shape that hid it),
and clearing the bench stranded `wb_live_q`, making the pane report *waiting on
you* forever with nothing to answer.

## How to run / verify

Use a **private** target dir. A shared one silently served a 3-test-short binary
in this session.

```bash
CARGO_TARGET_DIR=/home/parker/Work/terminal-delight/app/target-private cargo test --bin terminal-delight benchstore
```
```bash
CARGO_TARGET_DIR=/home/parker/Work/terminal-delight/app/target-private cargo clippy --locked -- -D warnings
```
```bash
cargo fmt --check
```

Confirm your tests are actually compiled in before trusting a count:

```bash
cargo test --bin terminal-delight -- --list | grep clearing_a_bench
```

See the store without a window:

```bash
terminal-delight conversation <root-session-id>
```

## Not done / next

- **Slice 3** — the key resolver. `tenancy_for(agent_pid)` first,
  `paneident::certain` as the fallback for a machine with no hook, neither means
  no filing. The reader is the tab-3 pane's, built on `bench/ledger-tenancy`.
- **Slice 4** — the `.swept` sentinel and the inbox drain, together.
- **Slice 5** — the wiring. **This slice must delete the narrow
  `#[allow(dead_code)]` on `benchstore`'s write path.** If it is still there
  afterwards it is hiding something rather than waiting for something.
- **Slice 6** — retention for `conversations/`, keyed on the conversation's own
  last activity.
- **Four screens held** for `docs/plans/workbench-drives-the-agent/` — the
  workbench/terminal decoupling, at its own Gate 1. They are designed and
  approved here, and drawn in `mockups/`.
- **Filed:** a signalled agent never lowers the alternate screen, so its bench
  never clears — issue #649.

## Watch out

- **`/clear` mints a NEW ROOT; `seq` counts continuations.** Getting this
  backwards produces a design that empties a bench on every compaction, with
  every step succeeding. The branch table is quoted in `benchstore`'s module doc
  from `scripts/td-agent-ledger` — read it there, not from anyone's summary.
- **Order by `turns.jsonl`'s `at_ms`, never by id.** Real ids are not monotonic.
- **The ledger outranks `paneident`** because one shell on this machine hosts two
  agents that `paneident` reports as one identity, both reading `declared`.
- **Two `paneident` tests fail on any box with agents running** — they walk the
  live process tree. Pre-existing, identical on clean main, already filed twice
  as #592 and #605 (which are duplicates of each other).
- **`cargo clippy -- -D warnings` does not lint tests.** It stayed green in this
  session while the test build was broken by a shadowed helper.
- **The primary worktree is shared and moves under you.** It went from a fix
  branch to `main` during this session, and another pane swept my untracked plan
  docs onto main mid-merge.

## Where it's recorded

- APES episode: `…/apes/projects/terminal-delight/episodes/2026-09-21-a-bench-belongs-to-the-conversation.md`
- APES kanban: one ticket closed with deliverable, one follow-up opened
  (`clear-the-bench-when-an-agent-is-killed-by-a-signal…`), cross-linked to #649
- Harvest: `handoffs/2026-09-21-bench-conversation-store.cdx`
- Plan + retro: `docs/plans/workbench-follows-the-agent/` (05-retro.md has twelve entries)
- Brief: `reports/2026-09-21-bind-the-bench-to-its-agent.html` — **never annotated**
- lean-ctx: session decision recorded. `ctx_knowledge` was not bound this session.
