# Handoff — the left bar's keyboard cursor, and a live cutover (2026-09-15)

## Status

**LANDED LOCALLY, INSTALLED, RUNNING — NOT PUSHED, NO PR.**
`bar/tree-keyboard-traversal` @ `8b1dbbb`, off `main` @ `8d680f2`.
`~/.local/bin/terminal-delight` → `td-8b1dbbb-tree-cursor`, and Parker's live
window is on it. 823 tests pass (2 ignored); fmt and clippy clean.

## What's done

- **`ctrl+alt+↑/↓` walk the left bar's tree**, one *drawn* row per press. `→`
  opens a folded branch and stays put, then enters its first child on the next
  press; on a task it activates the tab. `←` mirrors it: fold, then climb out.
  Gated on the bar being visible. *Verified:* 8 new `tree.rs` tests, and the ring
  photographed moving on a staged window.
- **`alt+w` closes the focused pane** — the rung between `ctrl+w` (tab) and
  `super+w` (Omarchy tile). Same path as the header `×`. *Verified:* the
  keystroke-ownership asserts in `pane.rs`, with `ctrl+w`'s werase pinned beside
  it so the two rungs stay distinguishable.
- **Both are in the F1 sheet**, nine locales.
- **The design decision worth keeping:** the cursor walks the output of
  `tree::rows` (extracted from the renderer into `Workspace::bar_rows`), so
  skipping a folded subtree is inherited, not reimplemented. No second copy of
  the fold rules exists to drift.
- **Live cutover onto a running session.** *Verified 9/9-style:* host pid
  1189484 unchanged, 25 shells held, window binary swapped, **zero agents lost**,
  and two orphan host panes re-adopted (17→19 tabs, 23→25 panes).

## How to run / verify

```
cargo test --manifest-path app/Cargo.toml
```
```
pgrep -af 'serve --session 1$'
```
```
ls -l ~/.local/bin/terminal-delight
```

The second is the one that matters during any binary work: that host owns every
terminal and every agent. Do not restart it.

## Not done / next

- **#420 — the cursor's step semantics are unproven under a human's fingers.**
  Synthetic (`wtype`) input produced a sequence `↓` cannot produce: three presses
  moved the ring one row, and by the sixth a branch had folded and the active tab
  had changed. Press it by hand ten times before trusting it; the issue carries
  the invalidation criterion.
- **#421 — `TD_DEMO_STATE` cannot photograph the left bar.** `left_bar_visible`
  returns false for scratch/demo windows, overruling an explicit `left_bar = true`.
  One run of three drew a bar anyway, which is unexplained and is the sharper half.
- **#422 — `shorts-pipeline` has three unpushed commits and `stash@{0}`** awaiting
  a decision (the filming pipeline looks unique to it).
- **No PR.** Pushing and opening one is the obvious next move, once #420 lands
  one way or the other.

## Watch out

- **Never restart the session host.** `td-36317b5-main serve --session 1`,
  pid 1189484, systemd-parented, up four days, holding 25 shells. The window is a
  disposable client; that process is not.
- **Rollback is one line**, then relaunch the window:
  ```
  ln -sfn /home/parker/.local/lib/terminal-delight/td-8d680f2-left-bar-manners /home/parker/.local/bin/terminal-delight
  ```
- **This repo has seventeen worktrees.** `~/Work/terminal-delight` was on
  `shorts-pipeline` — forty thousand lines behind `main`, no left bar at all —
  and a full turn of work was built there before the gap was spotted. Check the
  branch against `main` before editing, every time.
- **The `bartest` window beside Parker's is another agent's**, from
  `~/Work/td-bar-manners`. Leave it alone.
- Untracked `docs/media/clips/` in the tree is not ours.

## Where it's recorded

- APES episode:
  `~/BROWN-FAMILY-SPORTS/Software/apes/projects/terminal-delight/episodes/2026-09-15-the-left-bar-learns-the-keyboard.md`
- APES kanban: one ticket closed, three follow-ups opened (each mirrored to a
  GitHub issue, `follow-up` labelled, in `~/FOLLOWUPS.md`).
- lean-ctx: session decision + one finding (the verification-lever gotchas).
- File-memory: `the-window-is-swappable-the-host-is-not`,
  `a-demo-window-has-no-left-bar`, `the-branch-is-part-of-the-brief`.
- Session harvest: `handoffs/2026-09-15-left-bar-keyboard-traversal.cdx`.
