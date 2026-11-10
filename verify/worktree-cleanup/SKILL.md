---
name: verify-worktree-cleanup
description: "Model-check Terminal Delight's worktree cleanup (Settings › Code › Worktrees: review, then remove on a second press) against processes walking into a worktree mid-removal, with TLA+. Use before and after changing app/src/wtclean.rs or wt_look / wt_remove_press in app/src/settings_view.rs, or when asked whether cleanup can delete work or pull a tree out from under a process."
---

# Verify worktree cleanup

**The promise.** Removing a worktree never deletes uncommitted work and never pulls a tree out from under a running process.

**The harm if it breaks.** An agent's or shell's files vanish mid-edit, or a process is left standing in a deleted directory.

## Run it

```bash
verify/check.sh verify/worktree-cleanup/WorktreeCleanup.tla
verify/check.sh verify/worktree-cleanup/WorktreeCleanup.tla verify/worktree-cleanup/WorktreeCleanup-before.cfg
```
```bash
cd app && cargo test --locked --bin terminal-delight wtclean
```

The first is the code as it stands (`Quarantine = TRUE`): both rules hold and `findings.txt` is empty. The second is the code before 2026-11-10 (`Quarantine = FALSE`) and must still read `breaks (finding)` for both rules. The test `a_tree_someone_is_in_or_that_has_changes_is_put_back_and_a_clean_one_goes` runs the real removal on a real repository. Results read as in `verify/check.sh`: `holds`, `breaks (finding)` (listed in `findings.txt`), `BREAKS` (new), `FIXED?`. Read any break as a path with the pstack `/statechart` skill, using `view.ts`.

## What it found (2026-11-10)

- **NeverUnderAProcess and NeverDeletesWork broke** in the code before 2026-11-10. Path: you open the review: it qualifies → you press Remove → you press again (`/proc` re-read: nobody inside) → a shell or agent walks in → git finds it clean → (it writes a file) → git deletes it. The window between the re-read and the delete is milliseconds, and `git worktree remove` checks then deletes in two steps of its own.
- **The quarantine fix, now in `wtclean::remove`, holds both:** on the second press, `git worktree move` the tree to a name nobody else knows (a rename cannot be half done), then re-read `/proc` for the new path, then remove; if someone is inside, move it back. A process already inside is seen under the new name; nobody can walk in by the old path.

## Coverage

| Model action | Code |
|---|---|
| `Look` | `settings_view.rs` `wt_look` → `wt_gather` → `wtclean::scan`, `wtclean::judge`, `wtclean::process_cwds` |
| `Arm`, `Recheck` | `settings_view.rs` `wt_remove_press` (first press arms; second re-reads `/proc` and skips occupied trees) |
| `GitCheck`, `GitDelete` | `wtclean::remove` → `git worktree remove` without `--force` |
| `Move` | `wtclean::remove`: `git worktree move` to `<name>.td-removing-<pid>`, re-read `/proc` for the new path, `git worktree remove`, or move it back on either refusal |
| `Enter`, `Leave`, `Write` | any other process on the machine |

**Sync commit:** the commit that last changed this skill (`git log -1 --format=%h -- verify/worktree-cleanup/SKILL.md`). Run `git diff <that commit> -- app/src/wtclean.rs app/src/settings_view.rs` first; if the covered functions changed, re-check the table, update the model, then move the commit.

**Left out:** several worktrees at once (each is removed on its own), the merged/inactive/unchanged rules (they only decide what is listed), `git worktree remove`'s own locking.

## It can fail

The fix was broken on purpose: letting a process walk in while the tree is quarantined (drop `~Hidden` from `Enter`) → `NeverUnderAProcess` BREAKS on the quarantine config. The findings themselves are the demonstration for the current code.

## When you change the code on purpose

The quarantine is in (`wtclean::remove`, 2026-11-10). A change to the order of its steps (move, re-read, remove, move back) is a change to `Move`, `Recheck` and `GitDelete` in the model; check it before trusting the code. Never call `git worktree remove` on a tree at its own path again: the gap between the re-read and git's own check is the finding.
