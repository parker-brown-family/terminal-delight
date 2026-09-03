# Handoff — sticky-note pin + the dead right mouse button (2026-09-02)

## Status

**Landed, pushed, merged, installed.** Two commits on `main`:

- `c83f2df` — the pin feature, pushed direct to `main`.
- `8374836` — the fix that made it actually work, via **PR #270** (squash-merged,
  all five checks green, branch deleted).

`~/.local/bin/terminal-delight` → `td-8374836-notepin`. Working tree clean; the two
untracked `scripts/td-agent-vitals*` files are a concurrent session's, untouched.

## What's done

**Right-click a sticky note → a pushpin goes through it; the pane's tab wears 📌
until you right-click again.**

- `sticky::right_click(hit) -> Act` — anywhere on the paper pins, **including the
  peel corner** (right-click never destroys). A miss passes through to the pane.
  Pure function beside `click` / `alt_s` / `press`, so the rule is testable.
- **Only that gesture moves the pin.** `no_other_gesture_touches_the_pin` asserts
  `alt_s` and `click` never return `Act::Pin`; keystrokes need no assertion because
  `Press` has no pin variant.
- **Drawn** in `sticky::paint_pin` — rim, dome, specular dot, ferrule, tapered
  needle, split shadow. Head is the paper's **complementary hue** (`Paper::pin`), so
  it flags on any theme; a red pin vanishes on pink paper.
- **Tab badge** — `Workspace::tab_pinned_notes` counts pinned panes; rendered ahead
  of the agent roster so a finishing agent can't shuffle it out from under the eye.
  Steady, not pulsing. `📌2` when a tab holds two.
- **Persisted** — `SavedNote.pinned` (serde-defaulted, old state files still load)
  and `sticky::Saved { text, seed, pinned }`, which replaced the `(text, seed)`
  tuple across four call sites.
- Help modal row in all nine languages (`sticky_pin`, `k_rclick_note`).

**The pane never registered the right mouse button.** gpui's
`on_mouse_down(button, listener)` filters on `event.button == button`. The root
registered `Left` alone, so *both* right-button branches were unreachable — the pin,
and the pane's copy/paste tray, dead since `9b54766 MVP 0.1` while `lang.rs`
advertised it in nine languages. One-line fix, plus
`pane_registers_every_button_its_mouse_handler_branches_on`: reads `pane.rs`, pulls
the body of `on_mouse_down`, asserts every `ev.button == MouseButton::X` has a
matching registration in `render`.

**Verified:** 472 tests, `clippy --all-targets --locked -- -D warnings`, `fmt` — all
clean. The guard was proven by deleting the registration and watching it fail. Pin
rendering and the tab badge confirmed visually against the installed release binary.

## How to run/verify

```bash
cd /home/parker/Work/terminal-delight/app && cargo test && cargo clippy --all-targets --locked -- -D warnings && cargo fmt -- --check
```

Build + install (a merged PR changes nothing Parker sees until this runs):

```bash
cargo build --release && cp app/target/release/terminal-delight ~/.local/lib/terminal-delight/td-<sha>-<label> && ln -sfn ~/.local/lib/terminal-delight/td-<sha>-<label> ~/.local/bin/terminal-delight
```

See the pin without touching a real shell — hand-write a state TOML with
`pinned = true` under `[tabs.node.Leaf.note]`, then:

```bash
setsid env TD_DEMO=1 TD_DEMO_STATE=/path/to/demo.toml ~/.local/bin/terminal-delight &
```

Frozen lorem emitter in every pane, never persists, boots through the restore path.
Find its rect in `hyprctl -j clients`, `grim -g "<x>,<y> <w>x<h>" shot.png`, kill by
pid. A working demo state is in this session's scratchpad as `pin-demo.toml`.

Roll back:

```bash
ln -sfn /home/parker/.local/lib/terminal-delight/td-d959a1f-robots /home/parker/.local/bin/terminal-delight
```

## Not done / next

- **The physical right-click is unverified by machine.** No `ydotool`/`wlrctl`/
  `dotool` on this box and Hyprland has no click dispatcher, so no TD mouse gesture
  has ever been executed by a script. **#260** (commented with this session's
  evidence — one scripted right-click would have caught the registration bug at any
  point in the last year). Installing one tool unblocks it and every future mouse
  affordance.
- **#271 — sweep the tree for other dead button branches.** The new guard covers
  exactly one function in one file. Invalidate-first: if every `ev.button ==` branch
  already pairs with a registration, close `invalid`.

## Watch out

- **Esc stays load-bearing.** A posted note must never handle a key —
  `sticky::press` is a pure function so that rule is a test. The pin does not change
  it: pinning is mouse-only and never touches the composer.
- **A guard that greps its own source must be proven by breaking the code.** One
  that passes on the broken state certifies the bug. Both of `pane.rs`'s source
  tests were verified that way.
- **The pin must stay hard to remove.** Its whole value is that it cannot come off
  by accident. Do not add a keystroke, a peel side-effect, or a "clear all pins".
- Two other worktrees exist (`~/Work/td-reveal`, `~/Work/td-themes`) on their own
  branches — not touched here.

## Where it's recorded

- APES episode: `apes/projects/terminal-delight/episodes/2026-09-02-a-pin-in-the-sticky-note.md`
  (+ the `.cdx` harvest beside it)
- APES kanban: three tickets closed with deliverables, one follow-up open
- lean-ctx: session decision (resume breadcrumb)
- file-memory: `a-handler-branch-on-an-unregistered-key-is-invisible`,
  `td-demo-state-is-the-visual-verification-lever`
- GitHub: PR #270 (merged), issues #271 (new), #260 (commented)
