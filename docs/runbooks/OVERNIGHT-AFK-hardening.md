# Overnight AFK Hardening Run — Runbook

Drives the **GREEN** (and optionally **AMBER**) items of
[`docs/plans/HARDENING-1.0.md`](../plans/HARDENING-1.0.md) unattended, opening one
PR per task for morning review. Built to be **safe to leave running**.

## What it does

- **GREEN → ready PRs:** #136 (correctness matrix), #127 (alt-screen guard), #76
  (float popups), #75 (i18n dashboard strings). Code + tests, gated on local CI,
  pushed as a branch, opened as a normal PR.
- **AMBER → draft PRs (`needs-visual-verify`):** #87, #90, #137 (harness only),
  #138 (RSS harness only), #140 (Flatpak manifest only). Candidate branch +
  writeup; **never auto-merged**; body carries the visual-verify checklist.
- **RED → untouched:** #88, #23, #139. Documented for a Parker-present session.

## Guardrails (why it is safe to leave running)

1. **Worktree isolation, correct depth.** Each task runs in `git worktree add -b
   <branch> ../td-wt-<issue>` — a **sibling of the repo under `Software/`** so the
   `../../zed-upstream` gpui path-dep still resolves to the shared fork. Worktree
   is removed on completion. The main checkout is never edited.
2. **One writer, sequential.** Tasks run one at a time against a shared
   `CARGO_TARGET_DIR` (gpui compiles once; only app code recompiles per task ~15s).
   No concurrent build races on the shared fork.
3. **Local CI gate before any push.** `cargo fmt --check && cargo clippy --release
   --all-targets -- -D warnings && cargo test --release` must be **green**. Red →
   the branch is left local, a writeup is saved, **no PR is opened**. (`cargo fmt`
   is run *before* the check — scripted edits bypass rustfmt-on-save; this caused
   #123.)
4. **PRs, not main.** Nothing is pushed to `main`; nothing is force-pushed;
   nothing is auto-merged. Branches only + PRs for review.
5. **No AI attribution.** No `Co-Authored-By: Claude`, no "Generated with" footer
   (global rule). Author = Parker's git identity.
6. **Right gh account, automatically.** `~/bin/gh` selects `parker-brown-family`
   from the repo remote. No hand-switching.
7. **prepare-gpui per worktree.** `bash scripts/prepare-gpui.sh` (idempotent) runs
   in each worktree before building; shader patches live in the shared fork.
8. **Attempt cap + stop-on-red.** Each task gets bounded fix attempts; a task that
   cannot go green is abandoned cleanly (worktree removed, note saved) and the run
   continues to the next. One task never blocks the rest.

## Fire it

The driver is a Workflow script: `docs/runbooks/overnight-hardening.workflow.js`.

```
# from the repo root, in a Claude Code session:
Workflow({ scriptPath: "docs/runbooks/overnight-hardening.workflow.js" })
```

It runs in the background and posts a `<task-notification>` on completion. Watch
live with `/workflows`.

## Morning review

```bash
gh pr list --state open --json number,title,isDraft,headRefName \
  --jq '.[] | "\(if .isDraft then "DRAFT" else "Rment " end) #\(.number) \(.title)  [\(.headRefName)]"'
```

- **Ready PRs (GREEN):** review the diff + the new tests, merge if good.
- **Draft PRs (`needs-visual-verify`):** run the verify checklist in the body
  (e.g. `TD_ANCHOR_TOP=1` + open `vim` for #127; drag-select in FOCUS for #87)
  before marking ready.
- **#127 specifically:** confirm a full-screen TUI (vim/htop) with
  `TD_ANCHOR_TOP=1` no longer flips box-drawing.

## Abort / cleanup

```bash
git worktree list                       # see any leftover td-wt-* worktrees
git worktree remove ../td-wt-<issue>    # remove a stale one
git branch -D feat/<branch>             # drop a local branch if abandoned
```
