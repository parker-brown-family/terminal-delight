# Handoff — one table decides every keystroke on a pane (2026-09-19)

## Status

**Landed and installed.** Merged as `7f70c34` (pull request 609, squashed), branch
and scratch worktree deleted. Installed as `td-7f70c34-keylayer` with
`~/.local/bin/terminal-delight` repointed from `td-64feea5-benchwheel`. Issue 524
closed as completed. Nothing uncommitted from this thread.

New windows run the fix; panes already open keep the build they started on.

## What's done

`keylayer::route` is now the only decision for who owns a keystroke on a pane —
one ordered ladder of twelve layers, each rung declaring **what it claims**, which
is what lets a single ladder answer for escape and for typing without two tables
drifting:

```
Help · Face · Window · Paint · CtxMenu · HeaderMenu · Reader
     · PaneChord · Sticky · Rename · Bench · Terminal
```

- `TerminalView::on_key` went from ~430 lines of `if` to a 55-line `match`, with
  five named handlers under it (`reader_key`, `rename_key`, `pane_chord_key`,
  `terminal_key`, plus the existing `paint_key` / `sticky_key` / `bench_key`).
- `bench_key` reports what it took instead of calling `stop_propagation`;
  `Reading::Ignore` renamed `Reading::Pass` because the arm now hands the key
  down. `window_chord` moved from `workbench.rs` to `keylayer.rs`.
- **21 of 27 chords came back on the workbench face**: the header's inline rename
  box, `alt+s` / `alt+backspace`, `ctrl+w`, `ctrl+f`, `ctrl+x`, every
  `ctrl+shift` panel, the scrollback and selection keys, and `ctrl+c` reaching
  the agent whose turn is on the screen.

Verified by: 15 table tests (12 mutations killed), 2 source guards on the new
shape (4 mutations killed), 1382 unit + 30 integration tests, clippy
`--locked -D warnings`, rustfmt, CI green on all four checks. Merged tree hash
compared against the tested branch before building — identical.

**Not verified by hand.** The tables were executed; the wiring around them is
read. Nobody has pressed a key on the installed build.

## How to run/verify

```
rustc --test --edition 2021 -o /tmp/kl app/src/keylayer.rs && /tmp/kl
```
```
cd app && cargo test --locked && cargo clippy --locked -- -D warnings && cargo fmt --check
```

The two presses that settle it, in a **new** window (existing panes run the old
binary): `alt+k` to the workbench face, right-click a pane title, type a name,
`enter`. Then `ctrl+shift+b` and `ctrl+c` from the same face.

## Not done / next

- **#611** — a bare ←/→ on a reading bench now reaches the agent's line editor,
  where its caret is not drawn. A deliberate consequence of "the bench passes what
  it does not use", filed because the arrow case was never weighed separately from
  the `ctrl+c` case. APES: `decide-what-a-bare-left-right-arrow-…-mubb7ssz`.
- **#612** — `keylayer::paints` and `TerminalView::paint_key` agree only by a
  `debug_assert`, which the release profile compiles out. No drift today; no test
  either. APES: `pair-the-paint-overlay-s-claim-…-mubb7v0y`.
- The decision brief still shows three options with the middle one lit. It carries
  an as-built banner now, but the option cards were not rewritten.

## Watch out

- **`on_key` is guarded against looking at a key.** `on_key_decides_nothing_it_can_decide_in_the_table`
  fails on any `ks.key` / `key.as_str()` / `modifiers.` / `key_char` in its body,
  and on more than one `keylayer::route(` or `stop_propagation`. That is the point
  — a new binding is a rung in the table, not an `if` here.
- **`bench_key` must not stop propagation.** `the_bench_says_what_it_took_and_never_stops_the_event`
  fails on any `stop_propagation` in it, and on a `Reading::Pass` arm that stops
  returning `false`.
- **Do not put a `#[cfg(test)]` item at file scope in `pane.rs`.** Several source
  scans cut the file at the first `#[cfg(test)]`; a test helper above `mod tests`
  moves that cut and reds an unrelated test (`the_pane_root_takes_a_file_drop`
  went red over a drop listener nobody touched).
- `instance::tests::one_window_per_key_and_the_lock_dies_with_it` flakes under
  parallel load — a shared lock file. Passes alone and on re-run.
- The primary worktree is on a different session's branch
  (`fix/the-second-click-backs-out-to-your-branch`, behind main). This work was
  done in a throwaway worktree off `origin/main` and that worktree is gone.

## Where it's recorded

- APES episode: `…/apes/projects/terminal-delight/episodes/2026-09-19-the-bench-kept-the-keys-it-could-not-use.md`
- Session harvest: `handoffs/2026-09-19-one-table-decides-every-keystroke.cdx`
- Decision brief: `reports/2026-09-19-the-bench-keeps-your-keystrokes.html`
- lean-ctx: `ctx_session` decision breadcrumb, 2026-09-21
- file-memory: `a-cfg-test-item-moves-every-scans-cut-point`,
  `the-comment-explaining-an-order-is-the-test-for-your-new-one`,
  `extract-the-real-function-to-measure-it`, and an addition to
  `the-root-jail-follows-the-cwd-not-the-repo`
- Pull request 609 · issues 611, 612 · issue 524 closed
