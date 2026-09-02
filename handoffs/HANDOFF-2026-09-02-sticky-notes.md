# Handoff — sticky notes on a pane's glass (2026-09-02)

## Status
**Landed and deployed.** `origin/main` = `a47626c`, three PRs merged with all four
required CI checks green: **#256** (feature + pre-warp), **#258** (position),
**#259** (gesture exits). Deployed as
`~/.local/lib/terminal-delight/td-a47626c-sticky`. Nothing uncommitted of mine.

⚠️ The local `~/Work/terminal-delight` is **1 behind `origin/main`** and carries
another agent's uncommitted `mod usage;` + `app/src/usage.rs`. Left untouched
deliberately — pull when they're done.

## What's done
- **`alt+s` pins a handwritten note** to a pane's top-right; Enter, a second
  `alt+s`, or a click off the paper all post it; `alt+backspace` peels it.
  Persisted beside the pane's cwd in `PaneRestore`, so it survives a restart.
  *Verified:* whole lifecycle driven through the running binary and
  screenshotted, including that typing after posting reaches the shell.
- **Esc does NOT remove a posted note** — only the composer. *Verified:* on
  screen, and by `a_posted_note_claims_no_key_at_all`.
- **The note is pre-warped, not cut out of the barrel warp.** Paper takes the
  exact inverse per vertex, glyphs an affine pinned at the text block.
  *Verified:* `the_paper_is_drawn_exactly_flat` measures 0.00px residual against
  a Rust transcription of `fs_crt`.
- **gpui patch `0005-glyph-transform`** — an ambient
  `Window::with_text_transformation`, `crates/gpui` only. *Verified:* the release
  build renders rotated handwriting; that was the check that proved the patch was
  in the *release* profile and not just debug.
- Paper colour from the theme's primary text at full chroma, black ink, 15%
  see-through. Caveat (SIL OFL) bundled. Help rows in all nine languages.

## How to run/verify
```bash
cd ~/Work/terminal-delight/app && cargo test && cargo clippy --all-targets --locked -- -D warnings && cargo fmt -- --check
```
Build + deploy (the repo's own recipe; **build from a clean worktree** while the
shared tree holds foreign WIP):
```bash
cargo build --release && cp app/target/release/terminal-delight ~/.local/lib/terminal-delight/td-<sha> && ln -sfn ~/.local/lib/terminal-delight/td-<sha> ~/.local/bin/terminal-delight
```
Smoke-test the note without touching Parker's windows:
```bash
TD_SCRATCH=1 ~/.local/bin/terminal-delight
```
Then `alt+s`, type, `Enter`. Driving it from a script: `wtype -M alt -k s -m alt`
for chords, `grim -g "$at $sz"` for shots, focus by **PID** and **abort if
`hyprctl activewindow` reports a different pid** — without that guard a script
types into whatever happens to be focused.

Roll back: `ln -sfn ~/.local/lib/terminal-delight/td-9872d26 ~/.local/bin/terminal-delight`

## Not done / next
- **#260 — click-driven gestures never executed.** Unit-tested only; no
  pointer-injection tool exists on this box (no `ydotool`/`wlrctl`/`dotool`,
  Hyprland has no click dispatcher). Install one, run the five checks in the
  issue, expect to close it `invalid`.
- **#261 — ~3px glyph drift** on a strongly curved pane. Structural: sprites
  carry only an affine. Invalidation-first — try to *see* it at maximum warp
  before changing working code.
- Both carry the `follow-up` label and appear in `~/FOLLOWUPS.md`.

## Watch out
- **Esc is load-bearing.** Do not make a posted note handle any key. The whole
  design rests on it; `sticky::press` is a pure function so that rule is a test.
- **The cutout approach is dead** — don't reintroduce `register_note_cutout`. A
  post-process exclusion is a discontinuity in brightness *and* geometry, and
  sizing it only trades one artifact for the other.
- **A modal editor must handle its own chords ahead of its own capture.** `alt+s`
  shipped broken for one build because the composer sat above it and
  `EditBuffer::apply` drops alt chords — silently, from both sides.
- **The shared checkout pollutes gates.** Another agent's untracked file turned
  clippy red on their dead code and moved the test count. Build in a worktree;
  it needs a sibling `zed-upstream` symlink.
- Parker's running TD windows were on `td-f2ad3ad-sticky` at close — the deploy
  needs a window restart to take effect.

## Where it's recorded
- APES episode: `apes/projects/terminal-delight/episodes/2026-09-02-sticky-notes.md`
  (+ `.cdx` harvest beside it)
- APES kanban: one closed ticket, two `todo` follow-ups cross-linked to #260/#261
- lean-ctx: session decision (`ctx_knowledge` was not bound this session)
- file-memory: `invert-a-post-process-dont-carve-a-hole-in-it`,
  `a-green-unit-test-doesnt-prove-dispatch-order`,
  `check-dirty-tree-before-building` (updated)
- PRs: #256, #258, #259
