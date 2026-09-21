# Handoff — the comments shelf (2026-09-19)

## Status

**Landed and live.** Four PRs merged to `main`: **566** (the board), **572** (a
documentation correction), **574** (`alt+1..4` shelf chords), **595** (the
keystroke regression + the `+ write a note` row). Installed as
`td-6beb381-main`; `~/.local/bin/terminal-delight` repointed. The live session
was cut over — host `876777` never restarted, **24 shells before the window kill
and 24 after**. Nothing uncommitted.

## What's done

| Piece | Verified by |
|---|---|
| `Kind::Comment` on `Shelf::Comments`, tinted `Tint::Mine` → `Role::Human` | 1402 tests; photographed on the release binary |
| Persistence via the existing `.json` surface transport | `Origin::Person` + the `Surface::merge` rule, both unit-tested |
| `alt+m` opens the note box; **typing never does** | `what_shelf_you_are_on_never_catches_a_keystroke`, mutation-tested |
| `+ write a note` row, dashed, wearing `ALT+M` | photographed above two real notes |
| `alt+1..4` select a shelf | `every_shelf_has_a_chord_and_no_chord_belongs_to_the_window`, mutation-tested |
| The corner guard `benchdraw.rs` had promised for weeks | failed on the two real literals *before* they were fixed |

## How to run / verify

```bash
cd ~/Work/td-comments/app && cargo fmt --check && cargo clippy --locked -- -D warnings && cargo test --locked
```

**The CI gate is clippy WITHOUT `--all-targets`.** With it you get 9 pre-existing
test-code lints in `keylayer.rs`, `launcher.rs`, `surface.rs`, `workbench.rs` and
`main.rs` — none are from this work.

To look at the board on a throwaway window that cannot touch the live session:

```bash
XDG_STATE_HOME=/tmp/td-rig TD_SCRATCH=1 ~/.local/bin/terminal-delight
```

Then put a surface on its bench (a scratch window has **no pane id**, so the file
transport is unreachable there — drive it over MCP instead):

```bash
terminal-delight ctl --pid <window> mcp on && terminal-delight ctl --pid <window> mcp writes on
```

## Not done / next

- **#568** — select part of a comment card's text and copy just that. Parker asked
  for it; whole-body `copy` is what shipped. The bench has **no text-selection
  model for any kind**, so this is real work, not a tweak.
- **#569** — a comment loses its `you` across a restart. Honest (the file cannot
  vouch for a writer) but it means every row of an older board is unsigned. First
  place issue **483**'s claimed-writer field costs something.
- **#570** — a hostless pane cannot save a note, because `pane_id` comes from the
  session host. The box says so before you type and never swallows the draft.

All three are dual-written: GitHub issue ↔ APES kanban ticket, cross-linked.

## Watch out

- **`ctx_*` refuses sibling worktrees.** All work in `~/Work/td-comments` used
  native `Read`/`Bash`. Expect the same in any `td-*` worktree.
- **`cargo fmt` invalidates every prior `Read`**, so `Edit` fails after it. Source
  edits went through `python3` heredocs asserting `count(old) == 1` before
  replacing — that assertion caught two ambiguous matches.
- **Re-run mutation tests after every merge and every `fmt`.** One reported a live
  guard as *dead* because formatting had moved the text its mutation string
  matched.
- **`main` moves under you.** It moved twice mid-session; the second merge spliced
  two independently-added test functions into four interleaved conflict hunks.
  Rebuild each side whole rather than resolving hunk by hunk.
- **Before overwriting another agent's launcher cutover**, run
  `git merge-base --is-ancestor <sha> origin/main`. A `-flight`/`-glow` label is
  not evidence of unmerged work.
- **A plain relaunch does not adopt a surviving host** (#369). Use
  `TD_SESSION=1 ~/.local/bin/terminal-delight`. Recovery card:
  `~/Work/reports/TD-CUTOVER-RECOVERY.md`.
- One pane's replica grid diverged on attach — that is **#378**, open since
  11 September, not from this work. Evidence posted there.

## Where it's recorded

- **Episode:** `apes/projects/terminal-delight/episodes/2026-09-19-the-comments-shelf.md`
- **Harvest:** `handoffs/2026-09-19-comments-shelf.cdx` (400 K, 855 secrets redacted)
- **Plan + post-hoc score:** `docs/plans/comments-shelf/00-status.md` (6/10 against a predicted 5)
- **Brief, with its own correction banner:** `reports/2026-09-18-comments-shelf-and-spine-dry.html`
- **Screenshots:** `~/Work/reports/2026-09-18-comments-shelf/`
- **Cutover log:** `~/Work/reports/td-cutover.log`
- **lean-ctx:** session decision recorded (`ctx_knowledge` did not bind this session)
- **file-memory:** 6 new/updated notes under `~/.claude/projects/-home-parker-Work-terminal-delight/memory/`
