# Handoff — the bench acts on your click (2026-09-21)

## Status

Landed. `main` at **`4e4c2af`**, installed as `td-4e4c2af-main`, launcher
repointed. PRs **610**, **613** and **596** merged and pushed. Nothing
uncommitted. Three PRs remain open and none belong to this thread: 608 (closed
pipe), 591 (header number), 589 (resolver guards).

**The one thing not confirmed:** nobody has watched the dial work end to end. The
only run a person saw was Parker's, on a build without the fix.

## What's done

| Change | Verified by |
|---|---|
| A dial press types the command, answers the harness's own modal confirmation, and restores the draft with its caret | `workbench::dial_step` + `screenread::harness_confirm` unit tests; three source-guard mutations |
| The hold ends on the menu clearing, never on a clock; the screen read is three-valued | `dial_step` case table, incl. a picker that is not ours held past every deadline |
| An open dial menu closes on any click and on escape | `workbench::dial_dismisses` + the `Peel::Dial` rung |
| The overview latches the person's last turn; its fallback is readable | `screenread::human_message` over both windows; `ink_faint` not `th.faint` |
| Artifacts accept six spellings of their location and keep their extra keys | `surface` parse tests + the demo fixture now carries the breaking shape |
| Table columns sized from content, cells wrap | `benchdraw::column_shares` |
| Weights are a bordered readout, not chips; open tab borders its section; `next` → `steps`; routing lines behind `TD_VERB_PREVIEW` | source guard + `Group::label` test; three mutations |
| **The real picker is a fixture** — no `esc to cancel` footer, so "a menu is up" now reads the gutter mark too | `screenread::tests::the_real_picker_off_parkers_own_pane_is_recognised_and_driveable` |

## How to run / verify

```bash
cd /home/parker/Work/td-rodeo/app && CARGO_TARGET_DIR=/home/parker/Work/terminal-delight/app/target cargo test --bin terminal-delight
```
```bash
cd /home/parker/Work/td-rodeo/app && CARGO_TARGET_DIR=/home/parker/Work/terminal-delight/app/target cargo clippy --locked -- -D warnings && cargo fmt --check
```

The end-to-end check nobody has run, on a **restarted** window (running windows
keep their old binary; the host keeps the panes):

1. Put half a sentence in the bench composer of an agent pane.
2. Press a value on the **EFFORT** dial.
3. The harness raises `Change effort level?`; the window should press Yes and the
   half-sentence should come back with the caret where it was.

To see the routing lines again: launch with `TD_VERB_PREVIEW=1`.

## Not done / next

- **Nobody has watched the dial work.** Highest-value next action, and it is one
  restart.
- **The bench's own surfaces have never been photographed** — table, artifact
  card, YOU block, the new card chrome. Blocked on the rig, already filed:
  issues **586** and **492**.
- **Issue 618** (new, `follow-up`): the your-turn badge is blind to the harness's
  confirmation picker, because `wants_human` matches a footer that picker does not
  have. Fixture is already in-tree; the assertion that states the defect passes
  today.
- The verb-preview audit view is behind an env var. If it should come back as a
  gesture (hover a verb, hold a modifier), that is a better home than either
  extreme — raised in PR 613's body, not filed.

## Watch out

- **`git checkout <file>` as a mutation-test restore deletes uncommitted work.**
  It ate a fixture and a predicate change here. Commit first.
- **A filtered `cargo test` that matches no test prints `0 passed; 0 failed`** —
  which reads like a guard surviving a mutation. Check the count.
- Two `paneident` tests fail on this box on plain `main` (issues 592, 605). They
  are not yours; classify by re-running on main before blaming a merge.
- `~/Work/terminal-delight` (the primary worktree) is still checked out on
  `fix/the-second-click-backs-out-to-your-branch`, now merged. Other agents live
  there — it was left alone.
- Working in a sibling worktree costs a native `Read` per file because of the
  lean-ctx root jail. Set `LEAN_CTX_EXTRA_ROOTS` at `git worktree add` time.

## Where it's recorded

- **Episode:** `apes/projects/terminal-delight/episodes/2026-09-21-the-bench-acts-on-your-click.md`
- **Brief (drawn, annotatable):** `~/Work/terminal-delight/reports/2026-09-19-the-bench-acts-on-your-click.html`
- **Harvest:** `handoffs/2026-09-21-the-bench-acts-on-your-click.cdx` (349K, 6 facts, 841 secrets redacted)
- **lean-ctx:** 4 knowledge facts + the session decision
- **file-memory:** `a-slash-command-is-not-the-change`, `a-menu-is-a-stop-condition-not-a-clock`,
  `the-demo-seeded-only-the-spelling-that-worked`, `dpms-is-not-presence`,
  `commit-before-you-mutate`; updated `the-root-jail-follows-the-cwd-not-the-repo`
  and `verify-a-merge-against-the-merged-tree`
- **PRs:** 610, 613, 596 · **Issue:** 618
