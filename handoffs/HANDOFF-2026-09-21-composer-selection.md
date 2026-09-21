# Handoff — the workbench composer's selection (2026-09-21)

## Status

**Landed and merged to `main`.** Three pull requests, all green, all cleaned up:

| PR | What |
|---|---|
| [#626](https://github.com/parker-brown-family/terminal-delight/pull/626) | Audit of the decoupling against the 23-row text-entry scorecard |
| [#639](https://github.com/parker-brown-family/terminal-delight/pull/639) | Shift + any motion selects; the anchor model |
| [#646](https://github.com/parker-brown-family/terminal-delight/pull/646) | Retro: one door for the selection API |

Nothing of this thread is uncommitted. Both temporary worktrees (`td-shiftsel`,
`td-retro`) are removed and pruned; both branches deleted.

## What's done, and how it was verified

- **`Line.marked: bool` → `anchor: Option<usize>`**, stored unordered so a
  backwards selection that crosses its own origin works. `sel_range()` is the
  single place deciding whether a zero-width range is a selection.
- **`Edit::Move(Motion)` / `Edit::Extend(Motion)`** over one `Motion` enum.
  `line_edit` resolves the motion *before* consulting shift, so a motion added
  later cannot ship with a bare form and no selecting one.
- **`place(at)` is the only public way to move a caret**; `seek` is gone and
  `left`/`right`/`home`/`end`/`mark_all`/`clear_mark`/`take_marked` are private.
- **Copy and cut act on the selection** when there is one, the whole draft when
  there isn't.
- One bug found by reading rather than testing: history recall claimed `up`/`down`
  without consulting shift. Extracted to `recalls_history` so it could be asserted.

Verified three ways: **1474 tests** green on the merged tree (private target
dir), fmt + `clippy --locked -- -D warnings` clean; **seven mutations** of the
implementation, each required to fail its test, all caught; and the gesture
**photographed on the real build** — frames in `reports/2026-09-21-shift-selection/`.

## How to run / verify

```bash
cargo test --locked --manifest-path app/Cargo.toml
```
```bash
cargo clippy --locked --manifest-path app/Cargo.toml -- -D warnings
```
```bash
scripts/shift-select-manual.sh target/release/terminal-delight
```

**Use a private `CARGO_TARGET_DIR`** before trusting a test count. A shared one
silently dropped seven tests from a green run this session.

## Not done / next

- **The composer's MOUSE selection** — drag, double/triple-click, shift+click.
  Issue [#625](https://github.com/parker-brown-family/terminal-delight/issues/625)
  (retitled), APES ticket `give-the-composer-a-mouse-selection-…-mubnycwu`.
  `Hit::Composer` is absent from the bench's drag-start set
  (`Hit::Arm | Hit::OpenRow(_) | None`), and `shift+click` needs modifiers
  threaded through `bench_press`/`bench_release`. Plumbing, not design.
- **`shift+click` was deliberately not done alone** — it needs the same threading
  drag needs, and building that twice is worse than waiting.
- **`pageup`/`pagedown`** — reasoned out on #625 rather than guessed: a page is a
  *visual* quantity and `up`/`down` walk *logical* rows.
- **A repo-owned fake agent** — [#650](https://github.com/parker-brown-family/terminal-delight/issues/650),
  APES `ship-a-repo-owned-fake-agent-…-mubnyhx1`.
- Named by the audit, untouched: **#616** (the type-size ladder is inert at the
  default gauge) and **#617** (right-click opens the terminal's tray over the bench).

## Watch out

- **Two `paneident` tests are environment-flaky** (#605) — they walk live
  processes on a box running ~20 agents. Four runs this session: two red, two
  green, including on pristine `main`. Do not read a red run as your fault
  without classifying it.
- **A test rig's fake agent is a trap.** It must be a real executable (the host
  classifies on `comm`), launched by **absolute path** (the pane shell's rc
  rebuilds `PATH` and puts the inherited part last), and then **asserted** — a
  real agent draws the same composer and produces correct screenshots. This rig
  started a live Claude session with six MCP servers and passed. Kill the session
  **host** too; it is parented to systemd, not to the window.
- **`~/Work/terminal-delight` is shared.** It is currently on `main`, 15 behind,
  with two dirty files under `docs/plans/workbench-follows-the-agent/` belonging
  to the tenancy pane. Not mine, not touched.
- Two `http.server` listeners are up on **8961** and **8953** — other sessions',
  owners still alive. Left running.

## Where it's recorded

- APES episode: `apes/projects/terminal-delight/episodes/2026-09-21-a-boolean-where-two-carets-belong.md`
- Session harvest: `handoffs/2026-09-21-composer-selection.cdx` (426K, 707 secrets redacted)
- lean-ctx: 4 knowledge facts + the session decision breadcrumb
- file-memory: `a-shim-on-path-loses-to-the-pane-shells-own-rc`, `fixing-the-instance-keeps-the-trap`,
  `verify-a-brief-through-the-dom-not-the-screenshot`, plus updates to
  `a-peer-agents-message-is-a-claim-not-a-fact` and `the-radar-is-the-unfiltered-tool-log`
- The brief that started it: `reports/2026-09-21-workbench-text-entry.html`
