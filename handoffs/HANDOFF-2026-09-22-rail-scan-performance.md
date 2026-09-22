# Handoff — the rail's git crawler (2026-09-22)

## Status

**Landed.** PR [#684](https://github.com/parker-brown-family/terminal-delight/pull/684)
merged as `2d46e72` on `main`, 2026-09-22 16:03 UTC. CI Rust checks green (6m3s).
Branch `perf/the-rail-scan-stops-re-walking-the-tree`, one commit `49dc152`,
`app/src/engstate.rs` only (+362/−148). Nothing uncommitted.

A fast-follow review of the already-merged rail PRs #677 and #681, pointed at the
git crawler behind the top row.

## What's done

**The scan went from 2732 ms to 688 ms (3.96×) and its reading did not change.**

| what | how it was verified |
|---|---|
| Pipe deadlock fixed — `run_git` drains stdout on a thread *before* it waits | `a_wordy_git_is_still_measured`, verified to FAIL on the merged runner in 8.10 s |
| 15 ms poll tick → 150 µs–2 ms backoff | per-command timing: `rev-parse --show-toplevel` 15.4 ms → 2.9 ms |
| `parallel_map` — std-only `thread::scope` pool, capped at 8, one item at a time | `the_pool_returns_every_item_in_order`, 200 items under uneven work |
| 10 subprocesses removed (porcelain reuse + worktree-listing reuse) | `the_second_status_was_the_first_one_without_its_untracked_lines`, on a staged rename beside an untracked file |
| **the reading is identical** | both scanners compiled into one binary, same input: 228 fields, 9 ticker frames, landing list, badge |

Gates: `cargo fmt --check` clean, `cargo clippy --locked -- -D warnings` clean,
`cargo test --locked` 1572 passed / 0 failed / 6 ignored.

## How to run/verify

```bash
cd /home/parker/Work/terminal-delight/app && cargo test --locked engstate::
```
```bash
cd /home/parker/Work/terminal-delight/app && cargo clippy --locked -- -D warnings
```

To re-measure, or to prove a future change keeps the reading — the recipe is in
the memory note *Prove the reading did not change*:

1. `git show <old-ref>:app/src/engstate.rs` into a scratch dir.
2. Cut both copies at `// ---- the report ---` (the tail needs serde) and re-append
   the helpers `short_root`/`plural`/`panes`/`number` from after that marker.
3. `#[path]` both in as `old_`/`new_`, `rustc -O --edition 2021`.
4. Drive both over one `ScanInput`; diff `Debug` with `Instant`/`tv_sec`/`tv_nsec`
   lines filtered out.
5. Alternate the two for timing — never three passes of one then three of the other.

## Not done / next

- **#685** — `touched` counts a filename with a space twice. `git diff --name-only`
  leaves the space bare, `git status --porcelain` quotes it, so `sort`+`dedup`
  cannot match them; this feeds the collision detector and the landing list.
  `-c core.quotePath=false` fixes only the non-ASCII half. Real fix is `-z` on both,
  which changes the rename parse (`XY new\0old\0`, not `XY old -> new`).
- **#687** — collapse five per-checkout calls into one `status --porcelain=v2
  --branch` (verified to carry branch, oid, upstream and ahead/behind, omitting the
  last two when there is no upstream). **Measure before writing the parser** — the
  issue's own invalidation criterion may close it, since 555 ms of the remaining
  688 was one cold-cache worktree.

Both carry the `follow-up` label, appear in `~/FOLLOWUPS.md`, and have APES kanban
mirrors cross-linked in their `## Context`.

## Watch out

- **The eight-worker cap was chosen, not swept**, and the 2 ms poll ceiling means
  the slowest calls (`merge-tree` ~45 ms, `log --all`) are now polled ~25 times
  instead of 3. Neither was measured; neither showed up as a cost.
- **A fixture of untracked files in a subdirectory proves nothing** —
  `--untracked-files=normal` collapses a directory to one porcelain line. Mine
  printed 10 bytes on the first attempt. Put them at the repository root, and
  assert the fixture out-talks the 64 KiB pipe before asserting anything else.
- `~/Work` is outside the lean-ctx root, so a sibling worktree is native
  `Read`/`Bash` territory — and the no-paging rule does not come with the fallback.
- `~/Work/terminal-delight` is a **shared** worktree; its untracked `reports/` and
  `handoffs/` files belong to other sessions. Nothing here touched them.

## Where it's recorded

- APES episode: `apes/projects/terminal-delight/episodes/2026-09-22-the-scan-was-asleep.md`
- APES kanban: one done ticket + two follow-up mirrors (`…-mucvrocf`, `…-mucvs72g`, `…-mucvsto4`)
- lean-ctx: `ctx_session` decision recorded
- file-memory: four notes under `~/.claude/projects/-home-parker-Work-terminal-delight/memory/`
- session harvest: `handoffs/2026-09-22-rail-scan-performance.cdx`
- PR #684 · issues #685, #687
