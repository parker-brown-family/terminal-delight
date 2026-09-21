# Handoff — four ctrl+wheel size dials (2026-09-19)

## Status

**Landed and installed.** PRs [#549](https://github.com/parker-brown-family/terminal-delight/pull/549)
and [#597](https://github.com/parker-brown-family/terminal-delight/pull/597) are
merged; `main` is at `64feea5`. Built, installed as
`~/.local/lib/terminal-delight/td-64feea5-benchwheel`, and
`~/.local/bin/terminal-delight` repointed at it.

**A new window is required** — every running window holds the binary it exec'd.
The PTYs belong to the session host (`serve --session 1`), which was not touched,
so panes survive a window relaunch. Do not restart the host.

## What's done

ctrl+wheel now sizes the surface under the pointer, four ways:

| Pointer on | Sizes | Channel |
|---|---|---|
| Outer chrome — menu bar, tabs, left bar, gaps | the cabinet | `Scale` on outer |
| A pane's own header | that pane's header chrome | `Scale` pinned on the pane |
| The terminal grid | that pane's terminal text (reflows) | `TextSize` |
| The workbench | that pane's bench type ramp | `BenchSize` — new channel |
| The FOCUS reader | the mirrored pane's terminal text | `TextSize`, named outright |

- `size_dial_at(pos, content_bounds, on_bench) -> Option<GradeKey>` — pure
  resolver in `app/src/pane.rs`. `None` when the pane has never been painted.
- `TerminalView::size_by_wheel(ev, cx) -> bool` — the single chord. Both of a
  pane's wheel handlers ask it and **each halts itself**, because
  `scroll_by_wheel` is bubble-phase and `bench_wheel` is capture-phase.
- `Grade::bench_size: Option<f32>` — unset means *follow `text_size`*, resolved
  only in `Grade::bench_gauge()`. No session already on disk shrinks on upgrade.

**Verified:** `cargo fmt --check`, `clippy --locked -- -D warnings`, and
`cargo test --locked` (1371 pass, minus the pre-existing failures below). Nine
new tests. **38 mutations across the two PRs, every one watched fail**, including
the shipped bug put back.

**NOT verified by eye or by test:** nothing dispatches a real mouse event. Both
bugs in this feature were found by Parker using it. See issue #606.

## How to run/verify

```
cd /home/parker/Work/td-pane-textsize/app && cargo test --locked
```
```
cd /home/parker/Work/td-pane-textsize/app && cargo clippy --locked -- -D warnings
```

To rebuild and cut the launcher over (check the ancestor first, the symlink is
contested):

```
git merge-base --is-ancestor $(readlink ~/.local/bin/terminal-delight | grep -oP 'td-\K[0-9a-f]+') HEAD
```
```
cd app && cargo build --release
```
```
install -m 755 app/target/release/terminal-delight ~/.local/lib/terminal-delight/td-<sha>-<label>
```
```
ln -sfn ~/.local/lib/terminal-delight/td-<sha>-<label> ~/.local/bin/terminal-delight
```

The mutation runner used here is ad-hoc bash in this session's scratchpad; it
deserves to be a skill (noted in the episode's tooling ideas).

## Not done / next

Three filed follow-ups, each dual-written as a GitHub issue (`follow-up` label)
and an APES kanban ticket:

- **#605** — two `paneident` tests walk the machine's real process tree, so
  `cargo test` is red on any box with agent panes open. Green in CI's container.
- **#606** — no test dispatches a mouse event, so gesture routing is
  unobservable. This feature's two bugs are the proof.
- **#607** — `instance::tests::one_window_per_key_and_the_lock_dies_with_it`
  flakes 2 runs in 6 on pristine main; the suspicion is a forking test that skips
  `testsync::forks_and_locks()`.

Not attempted, deliberately: no keyboard chord for any of the four dials, no
reset-to-outer gesture, no separate dial for the left bar or tab strip (they are
the cabinet).

## Watch out

- **`~/Work/terminal-delight` is shared** — six `claude` processes had it as cwd
  when this started, and it is now on another agent's branch. This work was done
  in the sibling worktree `~/Work/td-pane-textsize` for that reason.
- **`ctx_read` is jailed to the SessionStart cwd**, so it refuses everything in
  a sibling worktree. Native `Read`/`Edit` work; for multi-site edits, a Python
  patch script with a `count(old) == 1` assertion per site beats N Read+Edit
  round-trips (the file got read 13 times before I switched).
- **The launcher symlink is contested** — another agent moved it mid-session.
  Check `git merge-base --is-ancestor` before repointing.
- **This branch (`fix/ctrl-wheel-over-the-bench`) was reset onto `main` after
  merge**, so it reports "ahead 7" of its own remote. That is a leftover pointer,
  not unpushed work; everything is in `main`.
- **The bench's capture-phase hook** (`pointer_hook` → `bench_wheel`) runs before
  every bubble listener and swallows turns. Any new pointer gesture on the bench
  has to be wired there too, not only at the pane root.

## Where it's recorded

- **APES episode:** `apes/projects/terminal-delight/episodes/2026-09-19-four-dials-and-the-handler-that-ate-the-fourth.md`
- **APES kanban:** one closed ticket (the feature) + three `todo` follow-up
  mirrors, each carrying its issue URL.
- **lean-ctx:** session decision recorded. `ctx_knowledge` was **not bound** this
  session — the durable facts went to APES and file-memory instead.
- **file-memory:** four new entries (capture-phase handlers, source-gate failure
  modes, splitting a setting as migration, fallbacks hiding their own setter)
  plus an update to `merged-is-not-installed`.
- **Session harvest:** `handoffs/2026-09-19-four-size-dials.cdx`
