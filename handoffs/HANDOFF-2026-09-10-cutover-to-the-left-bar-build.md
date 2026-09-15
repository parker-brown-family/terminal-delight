# Handoff — the cutover to the left-bar build (2026-09-10)

## Status

**LANDED AND INSTALLED.** `main` @ `36317b5`, pushed, nothing uncommitted.
`~/.local/bin/terminal-delight` → `td-36317b5-main`, so **every ctrl+alt+t opens
this build**. One window running it; the session host untouched since 13:38.
725 tests, 0 failures; fmt and clippy clean.

Merged this session: **#368** (a window records itself against the session) and
**#370** (tree shape sweeps + the bug they found + the use-case harness).
**#364** — the urgent "no installed build has both" issue — closed with evidence.

## What's done, and how each was verified

- **The cutover itself.** Built from `main`, installed by versioned path +
  symlink swap, both old windows retired, one relaunched. *Verified:* 9/9 checks
  — installed symlink, the live window's binary, host pid unchanged, 17 shells
  held, **panes lost: none**, `session-1.window` present with a matching ctl
  socket, and `terminal-delight mcp` completing its initialize handshake from
  inside a hosted pane.
- **A window records itself (#368).** `session-<key>.window` is what an agent's
  MCP relay reads to find its window; only the host wrote it, and the host
  cannot be upgraded without killing its terminals. The window now writes it too,
  beside the socket it greeted. *Verified:* a unit test against a hand-rolled
  host that greets and records nothing, plus the live appearance of the record
  against a pre-fix host.
- **Shape sweeps, and a real bug (#370).** Every combination of seven placements
  × eight fold states × three active tasks, holding three invariants. Two went
  red first run: a task carrying a *stale* project id was listed **nowhere**.
  Fixed — the loose bucket now holds anything hanging from a branch that is not
  in the session. *Verified:* 25 tree tests.
- **The use-case pass.** `scripts/td-left-bar-usecases.sh` — 18 checks through a
  real window's control socket: a pane joining a task, naming, grouping with a
  colour, folding, ungrouping, the bar's own state, all of it after a relaunch.
  *Verified:* 18/18 against the installed binary.

## How to run / verify

```
cargo test --manifest-path app/Cargo.toml
```
```
BIN=$HOME/.local/bin/terminal-delight ./scripts/td-left-bar-usecases.sh
```
```
ls -l ~/.local/bin/terminal-delight && pgrep -af "serve --session 1$"
```

The first is the suite; the second is the end-to-end pass (needs a Wayland
display, so not a CI gate); the third says which build is installed and whether
the host that owns the terminals is still up.

## Not done / next

- **#360** — a scoped strip resolves its tab-reorder drop slot against the
  unfiltered tab list. Still a mechanism, not a sighting.
- **#362** — the rest of the chrome's glyphs have never been checked against the
  fonts that exist; the check belongs in CI.
- **#363** — the desktop font is read once per process, so `omarchy font set`
  needs a relaunch; Omarchy fires a hook nothing of ours listens to.
- **#361** — the help modal has no row for ctrl+shift+B (needs all nine
  languages).
- **#319 stays open** — its "done when" wants the gate run, and "what the main
  space shows when a PROJECT is clicked" is still deliberately unanswered.

All four follow-ups carry both a GitHub issue and an APES ticket, cross-linked.

## Watch out

- **`ctl <verb>` with no `--pid` is a BROADCAST** to every window on the machine.
  It put three stray panes into the live session during this work.
- **Installing the same sha twice** replaces the inode under an already-running
  window; `/proc/<pid>/exe` then reads `… (deleted)`, and a script matching on
  that string will kill the window it just started. It did.
- **Never restart the host to upgrade a window.** A hosted pane is a child of
  the host; check `/proc/<pid>/status` before killing anything, especially when
  the agent doing the work is itself inside one of those panes.
- `grim` times out from an agent shell here, so a GUI change cannot be
  screenshotted — launch against a throwaway `XDG_CONFIG_HOME`, read the state
  file the app writes back, and leave the window up for a human eye.

## Where it's recorded

- **APES episode:** `…/apes/projects/terminal-delight/episodes/2026-09-10-left-bar-project-tree.md`
  (the cutover is its coda)
- **APES tickets:** the build and the install closed with deliverables; four
  follow-up tickets open, each naming its issue
- **lean-ctx:** `ctx_session` decision, updated at this tie-off
- **file-memory:** `left-bar-tree.md`, `td-wears-the-desktop-font.md`,
  `installing-under-a-running-window.md`, `screen-capture-is-unavailable.md`
- **Pre-cutover instance record:**
  `~/.local/state/terminal-delight/precutover-20260910-190621/`
- **Harvest:** `handoffs/2026-09-10-left-bar-and-cutover.cdx`
