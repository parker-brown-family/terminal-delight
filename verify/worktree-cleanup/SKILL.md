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
verify/check.sh verify/worktree-cleanup/WorktreeCleanup.tla verify/worktree-cleanup/WorktreeCleanup-quarantine.cfg
```

The first is the code as it stands (`Quarantine = FALSE`); the second is the proposed fix. Results read as in `verify/check.sh`: `holds`, `breaks (finding)` (listed in `findings.txt`), `BREAKS` (new), `FIXED?`. Read any break as a path with the pstack `/statechart` skill, using `view.ts`.

## What it found (2026-11-10)

- **NeverUnderAProcess and NeverDeletesWork break** in the code as it stands. Path: you open the review: it qualifies → you press Remove → you press again (`/proc` re-read: nobody inside) → a shell or agent walks in → git finds it clean → (it writes a file) → git deletes it. The window between the re-read and the delete is milliseconds, and `git worktree remove` checks then deletes in two steps of its own.
- **With the quarantine fix both hold:** on the second press, `git worktree move` the tree to a name nobody else knows (a rename cannot be half done), then re-read `/proc` for the new path, then remove; if someone is inside, move it back. A process already inside is seen under the new name; nobody can walk in by the old path.

## Coverage

| Model action | Code |
|---|---|
| `Look` | `settings_view.rs` `wt_look` → `wt_gather` → `wtclean::scan`, `wtclean::judge`, `wtclean::process_cwds` |
| `Arm`, `Recheck` | `settings_view.rs` `wt_remove_press` (first press arms; second re-reads `/proc` and skips occupied trees) |
| `GitCheck`, `GitDelete` | `wtclean::remove` → `git worktree remove` without `--force` |
| `Move` | not in the code yet: the proposed quarantine step |
| `Enter`, `Leave`, `Write` | any other process on the machine |

**Sync commit:** `62f87a5b`. Run `git diff 62f87a5b -- app/src/wtclean.rs app/src/settings_view.rs` first; if the covered functions changed, re-check the table, update the model, then move the commit.

**Left out:** several worktrees at once (each is removed on its own), the merged/inactive/unchanged rules (they only decide what is listed), `git worktree remove`'s own locking.

## It can fail

The fixed design was broken on purpose: letting a process walk in while the tree is quarantined (drop `~Hidden` from `Enter`) → `NeverUnderAProcess` BREAKS on the quarantine config. The findings themselves are the demonstration for the current code.

## When you change the code on purpose

Implementing the quarantine: model `Move` is the step to add in `wt_remove_press` (move, re-read `/proc` for the quarantine path, move back if occupied). Then set `Quarantine = TRUE` in `WorktreeCleanup.cfg`, expect `FIXED?`, and empty `findings.txt`.
