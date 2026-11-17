# Close-out, 2026-11 — every in-development line, and what happens to it

Difficulty, before: 7/10. A release built from two weeks of work that skipped CI, with six open review findings and a security one among them. Being wrong costs a bad release to every install.

Started 2026-11-17 from the job-search pane's handoff (`HANDOFF-2026-11-17-td-close-out-and-0.5.4.md`, in `~/BROWN-FAMILY-SPORTS/job-search-aug-2026/handoffs/`). The handoff planned two agents, triage then release lead. Under the house rule that fan-out is Parker's to start, one agent did both, in order.

## What the survey found

The ship set is one line, not five. `feat/high-trust-agentic` contains `feat/multibox`, which contains `dev/0.5.1`, which contains every November fix branch. Merging the high-trust head ships all of them. `deps/rio-vt-0.5.32` is one commit on main and merges cleanly beside them.

## Verdicts

Counts are as of 2026-11-17. "Merged another way" means `git cherry` shows no commit missing from main: the work reached main by a different PR.

### Ship in 0.5.4 (via the launch branch `release-0.5.4`)

| Line | Why |
|---|---|
| PR 1458 `feat/high-trust-agentic` | High trust agentic; contains everything below |
| PR 1457 `feat/multibox` | Agents across machines; inside 1458 |
| `dev/0.5.1` | November fixes; inside 1457 |
| `fix/settings-preserve-yours`, `fix/settings-menus`, `fix/settings-individual-glyphs`, `fix/floor-direction-cycle`, `fix/agent-enter-input` | inside `dev/0.5.1` |
| `deps/rio-vt-0.5.32` (local, 1 unpushed commit) | core bump; the advisory it answers is published |
| `feat/settings-search` (new) | Settings filter, agent settings under Agents, the six review fixes |

### Close with a note: merged another way (delete after 0.5.4 is out)

`research/adversarial-host-2026-11-10`, `feat/brief-reading-time`, `explore/t3-scuttle` (remote), `feat/sticky-stamp-every-skin`, `ctrl-c-saves-the-draft`, `fix/0.5-shipping-design-defaults`, `fix/save-window-workspace`, `feat/brief-launch-adapters`, `chore/zed-pin-maintenance`, `feat/terminal-linger-sliders`. Also `fix/restricted-review-completion`, `fix/restricted-hardening` and `fix/ctl-asks-before-widening`: their PRs (1096, 1095, 1088) merged into `dev/0.5.0` with different patch ids. `feat/terminal-agent-launch` is the same case: its PR 961 merged.

### Keep as an attempt (push to `refs/attempts/close-out-2026-11/<name>`, then delete the branch)

- `feat/jev-is-a-plugin-terminal-delight-never-assumes`: PR 706, closed. Jev is unplugged from Terminal Delight.
- Local-only branches with commits main lacks: `install/*` (the October pace and usage builds), `build/0.5.0-restricted*`, `fix/session-recovery`, `archive/td-efficiency-delivery`, `research/gui-in-the-tui`, `pr-870-review`, `preview/*`.
- Local-only branches with nothing main lacks (`consolidate/*`, `worktree-agent-*`, `t3/6-settings-replaces-config`, `pr-822`, `fix/one-line-of-work-is-not-1-lines`): deleted, nothing to keep.

### Keep as it is

- `cla-signatures`: the CLA bot's signature store, not work.
- PR 793 `research/terminal-core` and PR 752 `plan/program-pixels`: research briefs. Drafts are not releases, and closing them loses nothing.
- PR 844 `feat/mobile-hello`: the phone app, experimental and branch-only by design.

### Ask Parker

- **PR 1069 `feat/agent-management`** (recorded MCP handoffs, a 0.5.0 target, 653 commits behind main and conflicting). Rebuild it against today's main, or close it into an attempt ref?
- **The main checkout `~/Work/terminal-delight`** is on local branch `usage-pace-burndown` with one modified file, `app/src/usage.rs`: a rustfmt-only reflow of one assertion, dated 2026-10-17. No live pane works there. The roughly 260 other "uncommitted files" are untracked handoffs and plan folders. Left untouched; restoring it to `main` is his call.
- **`~/Work/td-t3-scuttle`** holds one untracked folder, `docs/plans/explicit-state-machines/`, and the photographing rigs other work still uses. Left alone.

### Worktrees

| Worktree | After 0.5.4 |
|---|---|
| `td-consolidate-lab/{floor-direction,settings-menus,settings-yours}`, `td-enter-recovery`, `td-phosphor-glow-lab`, `td-zed-pin`, `td-adversarial` | remove (clean, branch shipped or merged) |
| `td-multibox` | keep the `rig/` folder for the two-home Floor rig; the worktree can go |
| `td-rio-vt-0531` | remove once its commit is pushed |
| `td-consolidate-lab/td` (detached) | keep: the dev follower checks out there |
| `td-rel054` (this release) | remove after the tag |

## Release lead

The work is recorded in the release itself: the commits on `release-0.5.4`, the pull request into main, and the v0.5.4 candidate. Delete nothing until 0.5.4 is out.

Post-hoc difficulty: filled in when the release is out.
