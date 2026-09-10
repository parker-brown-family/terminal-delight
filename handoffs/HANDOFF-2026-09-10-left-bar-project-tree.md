# Handoff — the left bar: a two-layer project tree (2026-09-10)

## Status

**BUILT, PUSHED, PR OPEN, NOT INSTALLED.** Branch `left-bar` @ `d3933d9` in the
worktree `~/Work/td-left-bar`, off `a493359` (main, post client-server merge).
PR #359. Tests green: 711 pass, 0 fail.

**Why not installed, which is the one thing to read before installing it:**
`~/.local/bin/terminal-delight` points at `td-cf6d41f-hardened`, installed at
15:09 today by the in-flight hosted-mode hardening work (`cs/hosted-hardening`,
fixing #355 the MCP relay and #356 the divergence guard). This branch predates
those commits, so swapping the symlink to it would revert live fixes. The
versioned binary is built and sitting beside them at
`~/.local/lib/terminal-delight/td-d3933d9-left-bar` — one `ln -sfn` away — but
the correct order is: hardening merges to main → merge main into `left-bar` (or
land #359 after it) → rebuild → install once.

## What it is

The session drawn as a tree down the left edge of the window: **PROJECT** over
**INITIATIVE** over the tabs themselves, each tab holding its sub-terminals.
Issue #319's hierarchy, built. The initiative layer is the tab group that
already existed — same colour, same fold, same contiguous run — given a
`project` and read as what it always was.

The payoff is **scoping**: clicking a branch narrows the MOTHER BAR to it while
the tree stays whole. A strip carrying one push's worth of tabs is a strip that
stops wrapping, which was the whole complaint in #319.

## The invariants (they are the deliverable, not the layout)

1. Every task appears in the tree exactly once, whatever the scope, whatever is
   folded.
2. The branches holding the active task refuse to fold; activating a task from
   anywhere widens the scope to contain it. The strip always shows where you are.
3. A hidden tab can still shout: branch rows roll up 🤖 / ✅ / ❌ / 📌 and keep
   the animation for the loudest state; the strip carries a `⋯n` chip counting
   what the scope hides, lit when one of them needs input.
4. Unknown is not hidden: `left_bar` is `Option<bool>` on disk, resolved once at
   load. A pre-tree file opens as an unorganised list, not an empty window.
5. A grouped task's project is read from its group and never written twice.

## Where the code is

- `app/src/tree.rs` — **new**, ~600 lines. Pure: row builder, roll-up, scope
  rules, free of gpui and `Workspace`. 14 tests.
- `app/src/main.rs` — `Project` / `SavedProject` / `SavedScope` / `BarBranch` /
  `BarDrag`; `place_of`, `task_refs`, `file_task`, `file_initiative`,
  `set_scope`, `ensure_scope_shows`, `adopt_projects_from_dirs`;
  `render_left_bar`, `bar_row`, `branch_row`, `task_row`, `roll_badges`; the
  strip's scope filter and its `⋯n` chip; the project section in the tab tray.
- `app/src/pane.rs` — `ToggleLeftBar` (ctrl+shift+B), emitted from the pane
  because that is what has focus.
- `docs/plans/left-bar/` — the combined plan page and the status/score.

## How to look at it (without touching the installed binary)

A window is **already open on Hyprland workspace 8** running this build against
a throwaway config seeded with two projects, two initiatives and seven tasks.
To start another like it:

```
XDG_CONFIG_HOME=/tmp/claude-1000/-home-parker-BROWN-FAMILY-SPORTS-Software-terminal-delight/6fb46c4f-0f39-4742-b95d-1536c379d48b/scratchpad/smoke-config TD_NO_SESSIOND=1 /home/parker/Work/td-left-bar/app/target/release/terminal-delight
```

The seeded state and the launcher live in that scratchpad
(`smoke-left-bar.sh`); `TD_NO_SESSIOND=1` keeps it off the live session host.

## Verified

- `cargo test --manifest-path app/Cargo.toml` — 711 pass, 0 fail, 2 ignored.
- **Mutation-tested**, because a test that cannot fail is decoration: six
  load-bearing invariants were each broken on purpose and every one's test went
  red (force-expand, exactly-once, unknown-is-not-hidden, the waiting-agent
  count, git-root adoption, the out-of-scope roll-up). Script kept at
  `scratchpad/mutate.sh`.
- **End to end in the real app**: the release binary read the seeded tree,
  rendered it without a panic, and saved it back complete — `left_bar`,
  `left_bar_w`, `scope`, both `[[projects]]`, both groups' `project`, every
  task's placement.
- **Against the running old host**: the host merges layouts as untyped TOML, so
  the new keys survive a save made through `td-cf6d41f`. No `LAYOUT_SCHEMA` bump.

## What is NOT done

- **No screenshot.** `grim` times out on this compositor from an agent shell —
  three attempts, `-g` and `-o` both, so the rig was abandoned rather than
  diagnosed. The parked window on workspace 8 is the substitute.
- **No project overview.** Clicking a project scopes the strip; it does not fill
  the main space with cards. #319 left that question open and it stays open.
- **No help-modal row for ctrl+shift+B** — `lang.rs` forces all nine languages
  and a row in English alone is worse than none. The `⟩` handle on the strip is
  the discoverable path back meanwhile.
- **No keyboard navigation inside the tree** (arrows to walk rows).
- **No reordering of projects**, and no drag of a project row.

## Watch out

- Reorder-drag markers on the strip are computed from full tab indices while the
  strip may be showing a subset; dragging tabs around *while scoped* can land a
  tab in a surprising slot. Not hit in practice — reordering is usually done
  unscoped — but it is the known rough edge.
- `prune_groups` still deletes an initiative when its last task leaves it. That
  is existing behaviour, now reachable by dragging in the tree.
