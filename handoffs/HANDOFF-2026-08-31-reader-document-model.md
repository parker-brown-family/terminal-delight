# Handoff — the reader gets a document model (2026-08-31)

**Supersedes** `HANDOFF-2026-08-31-warp-suppression-pane-menus.md` from earlier
today, which covered only the first of these five PRs.

## Status

All merged into `main`. `~/Work/terminal-delight` is clean and level with
`origin/main`; deployed binary is `~/.local/lib/terminal-delight/td-281d357`.

| PR | What |
|---|---|
| #207 | Warp suppression missed three floating menus, one structurally |
| #213 | Alt+click copies a wrapped command as one unbroken line |
| #214 | Reader fills the width; reflows the mirror; dedents the transcript |
| #217 | **The document model** — `doc.rs`, the reader's real fix |
| #219 | Coverage for the two assumptions the vertical fill rests on |

## The arc, in one paragraph

Four menus bowed with the barrel warp because a hand-maintained suppression list
had drifted, and because a *pane's* own menus were unreachable from a predicate
over workspace fields. Then a copy affordance, which needed the pane's wrapped
rows rejoined. Then the FOCUS reader, which would not fill the screen — and three
successive fixes that each patched the previous one's misfire. That was the tell:
**the reader was rendering a rendering.** `doc.rs` replaced the photocopy with a
`Document` / `DocumentSource` / `layout()` seam, which also made the vertical fill
possible for the first time, because a document is not limited to one screenful.

## How to verify

```bash
cd ~/Work/terminal-delight/app
cargo test --release && cargo clippy --release --all-targets && cargo fmt --check
```

By hand, after restarting TD:

1. **Alt+click copy** — hold Alt over a wrapped command, chip appears, click,
   paste. One unbroken line.
2. **Reader fill** — Alt+R, drag the A—A slider left. Smaller text must show
   MORE, both axes.
3. **Reader history** — Alt+R opens at the newest content; scroll up for backlog.
4. **Warp menus** — the ▭ size popover, both `…` menus, the right-click menu.
   Glass flattens, clicks land true.

## Architecture, for whoever picks this up

`app/src/doc.rs` is the seam:

- `Document` — logical lines, real breaks only. No columns, no rows.
- `DocumentSource` — `PaneSource` reads the grid *including scrollback*, via a
  `RowBudget`.
- `layout()` — lays a document out at one surface's width, reporting the document
  position behind every visual row.

`Document::from_grid_rows` is the **only** place that guesses, and it only has to
because a TUI that hard-wraps its own output destroys the real breaks before the
terminal sees them. Quarantining it is the design: a source that knows its own
structure bypasses it entirely.

## Evening addendum — decisions landed, full mirror shipped (#221)

Q1 was answered: **FOCUS is a MIRROR** — the transcript source is **cancelled**
(APES ticket blocked with rationale; do not resurrect unless that decision
flips). Q2: the mirror now carries the **entire retained scrollback**, made
affordable by a content-generation document memo in the pane, a revision-keyed
layout memo in the reader, virtualised rendering (only visible rows become
elements), and **follow-bottom** (pinned to the newest row so the live prompt —
and what you type — stays in view; scroll up to release, return to re-arm).
Also: `budget_range` moved to i64 internals — `usize::MAX as i32` is −1, and
`RowBudget::all()` would have produced an inverted range and a BLANK reader.

First hands-on check after restart: Alt+R lands at the newest row; wheel scrolls
back through the whole convo; type into the prompt while the reader is open and
watch it echo at the bottom; Esc returns to the pane with the input intact.

**Hands-on verdict came back (late evening):** vertical fill approved; smallest
slider setting "pretty much just right"; largest "WAYYY too big (1.5–2x)". #222
capped `FZ_MAX` 3.0 → 1.6 (merged, deployed as `td-50e25e4`). The Ctrl+Alt+T
cascade did NOT reproduce on this run — still unfalsified, still on the list.

**#223 (same night): paging keys + URL chips.** PageUp/PageDown page the
scrollback in AGENT panes only (shells keep history-search); ctrl+Home/ctrl+End
jump to the scrollback ends in every pane; all four drive the FOCUS reader while
it is open (routed up like Esc — the pane keeps keyboard focus). Both defer to
alt-screen / mouse-mode apps. Alt+click copy now also chips a line-INITIAL URL
(http/https/file) — mid-prose URLs stay silent, elision still wins. Pure helpers
`read_nav_key` / `read_nav_target` carry the tests; 391 total. Deployed
`td-f545167`. Known truth for the wrapped-URL complaint: a URL the AGENT
hard-broke (narrow table cell, old narrow width in history) is unrecoverable
downstream — right-click → Open link and the chip both handle only what the
TERMINAL wrapped (WRAPLINE-stitched or line-initial).

**#225 filed (2026-09-01, morning after):** pre-resize narrow history reads
"crunched" and the reader reproduces it instead of healing it — the wrap-join
width test runs against the LIVE `src_cols` (doc.rs:149), so rows born at ~50
cols in a 108-col grid never read as full. Proposed: infer each run's
historical wrap width; reader-only, grid never rewritten. Falsifiable steps in
the issue. The new paging keys are what made old history visible enough to hurt.

## Not done / next

1. **#208** — `close_popups` and `warp::set_suppressed` enumerate the same overlay
   set by hand with nothing asserting they agree. That drift caused two of the
   four menu bugs.
2. **#220** — CI should fail when the passing-test count drops (the
   silently-disabled-test class).
3. **Ctrl+Alt+T cascade** — prime suspect is `Workspace::reap` ending in
   `cx.quit()`, which ends the *process*, not the window, while one process can
   host several windows. Falsify first: two windows in one process, exit the shell
   in one, see if the other dies. Not reproduced.
4. **`popup_open` has no test**, and there is no end-to-end check that the reader
   fills the glass — that wants a headless capture via `td-demo`.
5. **Reader perf under heavy streaming** — the document rebuild is memoised per
   content generation, but a firehose of PTY output while the modal is open still
   rebuilds ~10k lines per damage batch. If that ever shows up in `TD_LATENCY`,
   the next step is an incremental history document (history is append-only).

## Watch out

- **A test can be silently disabled and look identical to a passing one.** An
  earlier commit fixed a duplicated `#[test]` by deleting the wrong one, leaving
  the headline copy test with no attribute. It had not run for hours. Only
  clippy's "never used" warning surfaced it. If you resolve a duplicate attribute,
  check which function ends up bare.
- **Grid rows arrive space-padded.** Untrimmed, they read as full-width to the
  width test and the whole screen glues into one line. Silently. Pinned by tests
  in both `doc.rs` and `pane.rs`; do not "simplify" the trim away.
- **`cx.quit()` is process-wide**, not per-window. `window.remove_window()` is the
  per-window close.
- **Scrollback resize does not reflow** and cannot: alacritty only reflows rows it
  soft-wrapped itself (`Flags::WRAPLINE`). An agent that hard-wraps its own output
  emits real newlines, so old history legitimately keeps its old width. Not a bug.
- **A worktree of this repo needs a sibling `zed-upstream`** — `gpui` is a path
  dependency at `../../zed-upstream/crates/gpui`.

## Where it's recorded

- APES episode: `apes/projects/terminal-delight/episodes/2026-08-31-reader-document-model.md`
- Plans: `docs/2026-08-31-reader-document-model.html` (architecture, Q1–Q3),
  `docs/2026-08-31-one-click-copy-affordance.html`
- file-memory: `td-lives-in-work-not-bfs.md`, `check-dirty-tree-before-building.md`
