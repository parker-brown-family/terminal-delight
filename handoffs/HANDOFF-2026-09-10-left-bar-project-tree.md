# Handoff — the left bar: a two-layer project tree (2026-09-10)

## Status

**MERGED. NOT INSTALLED.** PR #359 merged; `main` is `bef1016`. Branch
`left-bar` @ `6936c43` in `~/Work/td-left-bar`, pushed, nothing uncommitted.
719 tests green, `cargo fmt` and `cargo clippy --all-targets` clean.

**Read this before installing anything.** `~/.local/bin/terminal-delight` still
points at `td-cf6d41f-hardened`, which carries the two hosted-mode regression
fixes (#355 the MCP relay, #356 the divergence-guard re-snapshot) that `main`
does **not**. Installing from `main` as it stands reverts live fixes on a
machine running agents through them. A concurrent agent staged a
`left-bar-hardened` combine branch — right idea, but cut before `main`'s tip, so
it is missing `6936c43` (the desktop-font default) and `eebeee3` (the pencil).

The whole sequence is **GitHub #364, `priority: highest`**: merge PR #358 →
merge `main` into the combine → build → symlink-swap → restart TD once.

## What's done

- **The tree.** PROJECT over INITIATIVE over the tabs, each tab a task holding
  its sub-terminals. The initiative layer is the tab group that already existed,
  given a `project`. New pure module `app/src/tree.rs` — rows, roll-up, scope
  rules and drop landings, free of gpui, 20 tests.
- **Scoping.** Clicking a branch narrows the MOTHER BAR to it; the tree never
  narrows. Three invariants pay for it: the tree is complete, the active task's
  branches refuse to fold and the scope always contains it, and branch rows roll
  up 🤖/✅/❌/📌 with a `…n` chip on the strip for what is hidden.
- **Filing and ordering, one drag.** The middle of a branch header means *join
  this*; a row's two halves mean *sit here*. Caret marks the seat, the drag chip
  names the verb.
- **The tabs sit over the terminals** — the strip is indented by the bar's width
  plus the same margins the screen uses.
- **Folding is real** — triangles drawn from rectangles, 15px targets, a
  fold-everything button.
- **TD wears the desktop's font** — `fc-match monospace`, what `omarchy font
  set` writes. An explicit `family =` in a theme still wins.
- **Two font bugs fixed:** the resolver now finds `JetBrainsMono Nerd Font` when
  asked for `JetBrains Mono` (it was silently running the whole UI on Liberation
  Mono), and the tab rename pencil moved U+270E → U+270F, because U+270E is in
  no font TD falls back to and has been invisible since it was added.

**Verification:** six of the tree's invariants were mutated on purpose and every
one's test went red (`scratchpad/mutate.sh`). End to end, a release build read a
seeded tree of two projects / two initiatives / seven tasks, rendered without a
panic, and saved it back complete. The font default was proved without touching
the real desktop — fontconfig reads its user file from `XDG_CONFIG_HOME`, so the
demo window's throwaway config could be moved to Liberation Mono while the real
desktop kept its face.

## How to run / verify

```
cargo test --manifest-path app/Cargo.toml
```
```
XDG_CONFIG_HOME=/tmp/claude-1000/-home-parker-BROWN-FAMILY-SPORTS-Software-terminal-delight/6fb46c4f-0f39-4742-b95d-1536c379d48b/scratchpad/smoke-config TD_NO_SESSIOND=1 /home/parker/Work/td-left-bar/app/target/release/terminal-delight
```
```
fc-list ":charset=25be" family
```

The first is the suite. The second opens a demo window against a throwaway
config seeded with a tree worth looking at, touching neither the real config nor
the live session host. The third is the glyph check any new chrome character
must pass — an empty result means no installed font can draw it.

## Not done / next

- **#364 (urgent)** — one install carrying both. Everything else waits behind it.
- **#360** — reordering a tab on a *scoped* strip resolves its drop slot against
  the unfiltered tab list. A mechanism, not yet a sighting.
- **#362** — the rest of the chrome's glyphs have never been checked against the
  fonts that exist; the check belongs in CI.
- **#363** — the desktop font is read at startup, so `omarchy font set` needs a
  relaunch. Omarchy fires a `font-set` hook nothing of ours listens to.
- **#319 stays open** — its "done when" asks for the gate run, and the project
  overview (what the main space shows when a PROJECT is clicked) is still the
  open design question. Clicking a project scopes the strip today.

## Watch out

- **Never install over the running binary** — versioned path plus `ln -sfn`.
- `prune_groups` still deletes an initiative when its last task leaves it. That
  is existing behaviour, now reachable by dragging in the tree.
- A **demo TD window is running on Hyprland workspace 1** against the throwaway
  config. Close it with ✕; it is not the installed build and never was.
- `grim` times out from an agent shell on this box (both `-g` and `-o`), so a
  GUI change cannot be screenshotted from here. Launch, read the state file the
  app writes BACK, and park the window for a human eye.

## Where it's recorded

- **APES episode:** `…/apes/projects/terminal-delight/episodes/2026-09-10-left-bar-project-tree.md`
- **APES ticket:** the build ticket closed with its deliverable; the install is
  `…-mtw6uypw` in `todo`, mirroring #364.
- **lean-ctx:** `ctx_session` decision recorded this session.
- **file-memory:** `left-bar-tree.md`, `td-wears-the-desktop-font.md`,
  `screen-capture-is-unavailable.md`.
- **Harvest:** `handoffs/2026-09-10-left-bar-tieoff.cdx`.
- **PR:** https://github.com/parker-brown-family/terminal-delight/pull/359
