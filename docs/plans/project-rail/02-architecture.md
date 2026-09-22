# Architecture: the project rail

## Fit

One new module and one surface swap; nothing else in the window learns
anything.

- `app/src/engstate.rs` — **new.** The model (`ProjectState`, `Checkout`,
  `RepoFacts`, `Foreign`, `Visitor`), the scan (`scan(&ScanInput)`, git in
  subprocesses), and the derivations (`badge()`, `frames()`, `sentence()`).
  std only, no gpui, so all of it runs under `cargo test` against a temporary
  repository with two worktrees.
- `app/src/main.rs` — the `Workspace` gains a map of readings keyed by
  project, a scan-in-flight flag and a frame index; two sweeps in the
  constructor (the scan, the ticker's clock); four small methods (gather,
  apply, tick, and the active project's state); and the top row's corner and
  middle change what they draw. The tab strip is built exactly as before and
  is shown when the tree is shut.
- Nothing in `pane.rs`, `host.rs`, the wire, the session file or the MCP
  surface changes. A pane contributes its live cwd through the accessor the
  dir-logo sweep already uses.

## Endpoints

None. The rail reads; it exposes nothing. (A `list_panes`-style MCP read of
the engineering state is an obvious later addition and is deliberately not in
this plan — the model was shaped so it could be serialised, and that is all.)

## Data

No new persisted data. The readings live in memory for the life of the
window and are rebuilt from the disk on every pass; a restart starts at
`SCANNING`. The session file is untouched.

What a reading holds, per project: the checkouts its panes operate in (root,
repository identity, branch or detached sha, dirty count, untracked count,
±lines, ahead/behind main, last commit time, writers), the repositories those
checkouts belong to (identity, name, main ref, worktrees on disk, the last
hour's commits in twelve buckets), the panes in no repository, the panes filed
here but working elsewhere, and the panes filed elsewhere but working here.

Every measured field is an `Option`. Unknown is not zero.

## Flow

```
constructor ─ spawn ─▶ every 2s: eng_scan_request()   main thread
                         │  nothing due?  → continue
                         │  gather ScanInput: every pane's tab, project, mode, cwd
                         ▼
                       background executor: engstate::scan(&input)
                         │  git rev-parse / status / diff / rev-list / log / worktree
                         │  per distinct cwd, per checkout, per repository
                         ▼
                       apply_eng(state)                 main thread, cx.notify()

constructor ─ spawn ─▶ every 6s: tick_eng_frame()      advance, cx.notify()

render ─▶ place_name(): PROJECT NAME + eng_badge()
      ─▶ middle = left_bar ? render_ticker() : tab_strip
                render_ticker(): rule · frames[eng_frame] · n/m · render_pulse()
```

A scan is only spent when the active project has no reading or its reading
is older than twenty seconds. One scan at a time. Switching projects therefore
costs at most one two-second beat before the rail speaks about the new place.

## External

`git` on `PATH`, invoked with `GIT_OPTIONAL_LOCKS=0` and an eight-second
ceiling per call, after which the child is killed and the field stays
unmeasured. `gh` for pull requests is slice 4 and is not reached yet.
`TD_RAIL_DEBUG=1` prints one line per landed scan to stderr with the count of
checkouts, repositories, foreign and visitors and how long the pass took.
