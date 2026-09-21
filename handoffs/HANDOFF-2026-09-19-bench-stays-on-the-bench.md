# Handoff — the workbench stays put when you answer (2026-09-19)

## Status

**Merged and unbuilt.** Pull request 559 went in as `f9becbc`, four commits,
verified as an ancestor of `origin/main` and read back out of it after two
further merges landed on top. Main has since moved to `569f452`.

Nothing a person sees has changed. This repository runs from a versioned
symlink, so the change is invisible until somebody rebuilds and repoints
`~/.local/bin/terminal-delight`. **The launcher was deliberately not repointed**
— the overseer pane (tab 29, pid 3643282) is batching a build, and its own
sticky reports a worktree debug build having seized the session.

## What's done

- **Answering a workbench surface no longer moves the pane's face.** Eleven
  lines out of `bench_act`'s `Dispatch::Tell` arm. Verified: the deleted
  comment and the `set_face` call are both absent from `origin/main`'s
  `pane/bench.rs`.
- **`nothing_in_the_bench_half_flips_the_pane_off_the_bench`** in
  `app/src/pane.rs`. Scans `app/src/pane/bench.rs` comment-stripped, fails on
  any `set_face`/`toggle_face`, names the owning function. **Mutation-tested** —
  the deleted line was replanted and the guard failed pointing at `bench_act`.
- **`docs/decisions/0001-the-machine-does-not-decide-an-interaction-is-over.md`**
  — the first decision record in this repository, carrying the rule, the twelve
  instances of the shape already in the tree, and how enforcement must be
  written.
- **`reports/2026-09-19-four-ways-off-the-bench.html`** — the drawn brief, six
  figures, annotatable.
- **Gates on the merged tree:** `cargo fmt --check` clean, `cargo clippy
  --locked -- -D warnings` clean, 1,336 tests green.

## How to run / verify

```bash
cd /home/parker/Work/terminal-delight/app && cargo test --locked --bin terminal-delight nothing_in_the_bench_half
```

To prove the guard rather than trust it, put the bug back and watch it fail:

```bash
git show f9becbc~4:app/src/pane/bench.rs | grep -n 'Answering is looking' -A 10
```

After the next build and relaunch, the manual check: on any agent pane's
WORKBENCH face, open a decision or question card and click an option. The pane
must **still** be showing the workbench, the card must record the answer, and
the live strip must show the agent taking it. `alt+k` and the TERM chip must
still leave. A question read off the terminal (one carrying a live cursor)
behaves identically — it always did, and collapsing the two onto that behaviour
is the whole change.

**What would prove it broken:** the pane turns to TERM on a click; or the
answer stops reaching the agent at all, which would mean the deletion took the
`bench_deliver` call with it rather than only the face flip.

## Not done / next

- **Build and install.** With the overseer. Merged is not installed here.
- **GitHub issue 565** — the overview's card scroll resets when a newly-arrived
  reply replaces the stand-in mid-read. Same shape, deliberately left on
  Parker's call (*"leave it for now - scream test"*). It has a date: no
  unprompted complaint by **19 November** closes it `invalid` and amends the
  open clause in decision record 0001. APES mirror ticket
  `scream-test-…-mu81jrpf`.
- **Undecided, and said so in the record:** whether the rule generalises beyond
  this repository into the machine-global agent doctrine.

## Watch out

- **The guard scans the whole file.** It can go red on a branch that never
  intended to touch the face — any new `set_face`/`toggle_face` in
  `pane/bench.rs`. That is the guard working, not a flake. The fix is to move
  the gesture into `pane.rs` beside `alt+k` and the TERM chip; the reasoning is
  in decision record 0001.
- **Several worktrees are in `pane.rs` and `pane/bench.rs` at once.** Main moved
  twice during this session and touched both files. Merge `origin/main` into
  your branch and re-run the gates there before handing over — a whole-file
  guard is meaningful only on the merged tree, and bare `git merge` merges the
  *branch's* upstream, not main.
- **`/home/parker/Work/td-bench-face` is still on disk**, merged, kept only
  because the decision record's `file://` link points into it. Safe to remove
  once main is checked out somewhere readable.
- **`reports/2026-09-19-four-ways-off-the-bench.html` is untracked here** in the
  shared worktree and also committed in main. The untracked copy is a duplicate
  and disappears on the next pull.
- The terminal-delight MCP server disconnected for this session when the window
  swapped to `td-c63d95b-glow`. The file transport under
  `~/.local/state/terminal-delight/surfaces/<session>/<pane>/` is the fallback
  and is what carried the overseer's final card.

## Where it's recorded

- **APES episode:** `projects/terminal-delight/episodes/2026-09-19-four-ways-off-the-bench.md`
- **APES tasks:** `answering-a-surface-…-mu7vj7ib` (done),
  `decision-record-0001-…-mu7y2ikb` (done), `scream-test-…-mu81jrpf` (backlog)
- **lean-ctx:** session decision + one pattern finding
- **file-memory:** `the-machine-does-not-decide-an-interaction-is-over.md`,
  `verify-a-merge-against-the-merged-tree.md`
- **Harvest:** `handoffs/2026-09-19-bench-stays-on-the-bench.cdx`
- **PR:** 559 · **Issue:** 565
