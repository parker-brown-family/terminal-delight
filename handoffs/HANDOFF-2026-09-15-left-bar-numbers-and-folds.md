# Handoff — the left bar's numbers, its ring, and a fold that folds (2026-09-15)

## Status

**LANDED LOCALLY, LIVE ON THE DESKTOP, NOT ON MAIN, NOT PUSHED, NO PR.**
`bar/tree-keyboard-traversal` @ `7d65624`, 4 commits ahead of `origin/main`
(`8b1dbbb` is the previous session's, then `de475ae`, `d980279`, `7d65624`).
`~/.local/bin/terminal-delight` → **`td-49deaf1-all`**, window **pid 1012028**.
Cut over twice today — first onto `td-4491953-ring-keys` (this session, 17 agent
sessions in flight, nothing lost), then onto `td-49deaf1-all` by another agent
carrying #433 on top; see the update below.

The installed binary is a **composite**: this branch merged with `a3bbefa`
(`install/bay-and-keys`, another agent's unmerged bay work, which was what was
already running). That merge lives on the local branch `install/ring-keys` and
exists only so the cutover did not regress anything Parker had. **The PR branch
is `bar/tree-keyboard-traversal`; do not push `install/ring-keys`.**

### Update, 13:40 — a second cutover, by another agent

Another session (`terminal-delight-a3`) rebuilt `install/bay-keys-twin` off
`install/ring-keys` @ `4491953`, built it as **`td-49deaf1-all`**, and cut the
window over again (pid 1012028). Verified independently, not taken on trust: all
four commits (`7d65624`, `d980279`, `de475ae`, `8b1dbbb`) are ancestors of
`49deaf1`; the host is still pid 6983 on `td-a3bbefa-bay`, now 72 pty descriptors
across 24 children (+4, their duplicate-leaf repair giving demoted leaves shells
of their own); the session file holds 24 pane_ids, all distinct, where it
previously had one id three times and two ids twice.

**It nearly went the other way.** `install/bay-keys-twin` had been reset past the
merge that carried this branch, and a build from it would have silently removed
six working chords. Caught by checking the branch rather than the agreement —
see the file-memory `watch-the-branch-not-the-intention`.

**`origin/main` is now `188d8b4` (#433) and carries NONE of these four commits**,
while the running binary carries all of them. This branch is 4 ahead of main and
13 behind it. The desktop is downstream of the install branch, not of main.

## What's done

- **`Ctrl+Alt+1…9` jump the cursor to a top-level branch** — projects in stored
  order, then the initiatives that hang from no project (`tree::top_branches` /
  `nth_top_branch`). Nine, because a tenth needs a chord that waits for a second
  digit. A loose task at depth zero takes no number on purpose. *Verified:* a
  test over the shape of Parker's own session, mutation-checked.
- **`Ctrl+Alt+↑/↓` wrap** — `tree::step` is a modular walk over the same `stops`
  list. *Verified:* the walk test became bounded and asserts both wraps.
- **`Ctrl+Alt+←` collapses any collapsible row, including the branch holding the
  active tab.** This is the real change; see below.
- **The cursor scrolls into view** (a tracked `ScrollHandle` + `bar_reveal`), and
  **`Ctrl+Alt+←` seeds a cursor from cold** the way `→` already did.
- **The F1 sheet** carries `Ctrl+Alt+1…9` in all nine languages.

### The decision worth keeping

`tree::rows` force-expanded the active task's branches **at draw time**, so a
fold of the branch you work in wrote `collapsed = true` and was overruled on the
next frame — dead by keyboard *and* by clicking the header, on the two rows
nearest the hands (here: TERMINAL DELIGHT and FEATURES). The rule moved to
`Workspace::reveal_active_branch`, called from the five paths where a *different*
tab becomes active — `activate_tab`, `new_tab`, `new_tab_in`, the pane-click
focus, restore — and deliberately not from the reorder/removal paths. `rows()`
lost its `active` argument rather than ignoring one, so drawn state and stored
state now agree. Safe because the mother bar filters by **scope**, never by fold,
and `ensure_scope_shows` keeps the active tab on the strip.

## How to run / verify

```
cargo test --manifest-path app/Cargo.toml
```
```
python3 ~/BROWN-FAMILY-SPORTS/Software/apes/projects/terminal-delight/episodes/tools/numbers.py
```
```
pgrep -af 'serve --session 1$'
```

827 tests on the branch, 864 on the composite; `fmt`, `check --locked`,
`clippy --locked` all clean. The second command above is the useful one: it reads
the live session file and prints the number map the app would compute.

**The fold verifies without a screenshot.** `toggle_branch` saves, so a real
collapse appears within a second as `collapsed = true` in
`~/.config/terminal-delight/sessions/1.toml`. Watch every branch, not only the
top-level ones — FEATURES is a group *under* project 1 and is the more
interesting of the two.

## Not done / next

- **Nobody has pressed the keys on screen.** The ring, the numbers and the fold
  are argued from tests, mutation checks and the state file. This is the one
  thing to do first.
- **#431 — the numbers are invisible on the bar.** Nothing but the F1 sheet says
  `TERMINAL DELIGHT` is `1`. Carries its own invalidation criterion: if a column
  of numerals crowds an 11px row, drawing them is the wrong answer.
- **#432 — nothing guards the five reveal call sites** among fourteen
  `self.active =` assignments. One of the five was already missed once, inside
  this session (`new_tab_in`, fixed in `7d65624`). Do **not** fix it with a shape
  test naming the five.
- **#420 — the cursor's step semantics under fast input.** Still open; its repro
  moved under it (↓ at the end now wraps), and there is a comment on the issue
  saying how.
- **#442 — the running binary is the only place `main` and this work are
  joined.** Filed with the one-line check: `git log --oneline
  <install-branch>..7d65624`, empty means nothing was dropped. Nearly fired once
  already today.
- **No PR, and now 15 commits behind `main`.** The previous session parked it
  pending #420; that is still the only thing blocking it. Note the odd state this
  leaves: the work is LIVE on Parker's desktop (via the install branch) while
  `origin/main` carries none of it, so "shipped" and "running" have come apart.

## Watch out

- **Never restart the session host.** `td-a3bbefa-bay serve --session 1`, pid
  6983, holding 72 pty descriptors across 24 children (it was 60/20 before #433's
  repair gave four demoted leaves shells of their own). It has survived two
  cutovers today and is four days old. The window is disposable; that is not.
- **Rollback is one line**, then relaunch the window:
  ```
  ln -sfn /home/parker/.local/lib/terminal-delight/td-a3bbefa-bay /home/parker/.local/bin/terminal-delight
  ```
- **A cutover must focus the window first.** `hl.dsp.window.close()` takes the
  ACTIVE window; this run aborted on its own guard because focus had moved to
  Chrome. `hl.dsp.focus({window = "pid:N"})` — and `hyprctl eval` prints `ok` for
  anything, so read `hl.dsp.*` signatures out of a deliberately wrong call's error.
- **The MCP relay drops on a bounce** (#391) for sessions already running;
  `terminal-delight ctl mcp rpc … --pid <window>` is how to read the census after.
- Untracked `docs/media/clips/` in this worktree is not ours.

## Where it's recorded

- APES episode:
  `~/BROWN-FAMILY-SPORTS/Software/apes/projects/terminal-delight/episodes/2026-09-15-numbers-a-ring-and-a-fold-that-folds.md`
- APES kanban: one ticket closed, three follow-ups opened, each mirrored to a
  GitHub issue (`follow-up` label): #431, #432, #442. Note `followups-gen` reads
  `gh search issues`, whose index lags creation by about a minute — verify the
  ROW (`grep -c "#442" ~/FOLLOWUPS.md`), not the exit code.
- lean-ctx: session decision recorded (`ctx_knowledge` is not bound in this
  build — gap flagged).
- File-memory: `a-rule-enforced-at-draw-time-overrules-the-gesture`,
  `closing-the-right-window-needs-a-focus-first`,
  `mutate-one-at-a-time-or-one-mutation-hides-another` (CORRECTED — the jobs
  were not reaped; the wait condition was unsatisfiable),
  `watch-the-branch-not-the-intention`,
  `followups-gen-lags-the-search-index`.
- Session harvest: `handoffs/2026-09-15-left-bar-numbers-and-folds.cdx`.
