# Handoff — strip scope and the bench dials (2026-09-19)

## Status

All landed. Three pull requests merged to `main`, CI-green on each:

| PR | merged as | what |
|---|---|---|
| [#546](https://github.com/parker-brown-family/terminal-delight/pull/546) | `19baa8b` | the tab strip opens on the group you are in |
| [#553](https://github.com/parker-brown-family/terminal-delight/pull/553) | `ecf1bd2` | the MODEL / EFFORT dials stay readable mid-turn |
| [#556](https://github.com/parker-brown-family/terminal-delight/pull/556) | `fcfbaa6` | a dial change sneaks behind the draft instead of sending it |

Installed at 21:21 as `td-fcfbaa6-main`. The live symlink has since moved on to
`td-4e3a1d7-fold` (#582) — **that build contains all three**, verified by
`git merge-base --is-ancestor` on each merge commit and by grepping
`Scope::Branch`, `pub fn dial_ink` and `pub fn aside_bytes` in its tree. Nothing
uncommitted; the working branch `bench/dial-sneaks-behind-the-draft` matches its
remote and is merged.

## What's done

**The strip carries one branch.** `tree::Scope` gained `Branch` as its `Default`
— it names no id and resolves against the active task every frame, so it cannot
go stale, cannot be emptied by a delete and cannot be restored onto a session it
no longer describes. The chip over the tree names that branch instead of reading
ALL; one press widens to the whole session and one press comes back. The scope is
no longer persisted at all (`SavedScope` parses old files, is written as `None`,
ignored on load) because every state file written while `All` was the default
says `All`, and no loader can tell that from a choice. An emptied pin falls back
to the active task's branch, not to every tab.
*Verified:* 5 tests in `tree.rs` (pure module, `rustc --test tree.rs` ≈ 1s), three
mutations each watched failing; full gates green twice, before and after merging
`origin/main`.

**The dials stay readable.** `benchdraw::dial_ink(known)` takes no liveness
argument, so the value's ink answers one question only — did anybody choose this.
Pressability is drawn by the caret (0.9 lit / 0.35 quiet) and the cursor.
*Verified:* a legibility floor plus a comment-stripped source scan of `dial`'s ink
line; both watched failing (old `match (live, known)` restored; inherited alpha
dropped to 0.2).

**A dial change no longer sends your draft.** `workbench::aside_bytes` compiles
one write: caret to column zero, `replace_bytes()` (ctrl+k), the command alone
with its own `\r`, the draft typed back, caret restored. `wb_compose` is never
touched, so the prompt stays on screen.
*Verified:* four tests in `workbench.rs` including the two orderings that make it
a fix (kill before the command; retype after its return), and a source scan of
`bench_dial_pick` — the defect was a call site. Both watched failing.

## How to run / verify

```bash
cd /home/parker/Work/td-strip-group/app && cargo fmt -- --check && cargo clippy --locked -- -D warnings && cargo test --locked
```
```bash
cd /home/parker/Work/td-strip-group/app/src && rustc --test tree.rs -o /tmp/tree-test && /tmp/tree-test
```
```bash
cd /home/parker/Work/td-strip-group && cargo build --release --locked --manifest-path app/Cargo.toml
```

To redeploy: copy `app/target/release/terminal-delight` to
`~/.local/lib/terminal-delight/td-<sha>-<branch>` and repoint
`~/.local/bin/terminal-delight` at it. A running window keeps the binary it
started with — a real quit and relaunch is required.

## Not done / next

Three follow-ups, each a GitHub issue with an APES kanban mirror:

- **[#557](https://github.com/parker-brown-family/terminal-delight/issues/557)** — a pasted image does not survive the aside's erase. Has an invalidation check: if Claude Code binds attachments to the turn rather than the input line, nothing is lost and it closes `invalid`. Run that check first.
- **[#593](https://github.com/parker-brown-family/terminal-delight/issues/593)** — a settled decision card stays approvable, and the press lands in whichever agent owns the pane now.
- **[#594](https://github.com/parker-brown-family/terminal-delight/issues/594)** — `$TD_TAG` is unset in agent panes, so a `[workbench:<tag>]` line cannot be verified as the operator's. Highest of the three: it is the protocol's own defence with no input.

## Watch out

- **The composer is a MIRROR.** Anything typed at the agent that the person did
  not press enter on must go through `aside_bytes`, never `bench_say` — the
  latter puts its argument in the composer, which holds their unsent prompt, and
  sends it.
- **Main moves fast.** Two of the three branches conflicted mid-build because
  another agent's work landed while gates ran (#545, #554, #548). Fetch again
  immediately before opening a pull request and again before merging.
- **The installed symlink moved three times in one evening.** Before hunting a
  regression, check what `~/.local/bin/terminal-delight` points at and whether
  that sha contains the fix — `git merge-base --is-ancestor <fix> <installed>`.
- `~/Work/td-strip-group` is a worktree this thread created, still present with a
  warm target dir. Safe to remove (`git worktree remove`) once nobody wants the
  build cache.

## Where it's recorded

- APES episode: `apes/projects/terminal-delight/episodes/2026-09-19-a-fixed-default-answers-neither.md`
- Session harvest: `handoffs/2026-09-19-strip-scope-and-bench-dials.cdx` (229.6K, 7 facts, 596 secrets redacted)
- lean-ctx: session decision + one finding (`ctx_knowledge` was not bound this session)
- file-memory: `the-bug-may-be-this-afternoons-decision`, `legibility-is-not-permission`, and an addition to `the-composer-is-a-mirror-not-a-text-area`
- CHANGELOG: three entries under `[Unreleased]`
