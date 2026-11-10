---
name: verify-overlay-stack
description: "Model-check which Terminal Delight surface is on top, which one a typed letter reaches, and which one Esc closes, when several are raised (Settings, the composer, a pane's design menu), with TLA+, plus replays on the real window. Use before and after changing the render order in app/src/main.rs, close_popups, the capture-phase key chain, or any surface a chord can raise over another."
---

# Verify the overlay stack

**The promise.** What a chord raises is what you see on top; a letter you type goes to the surface on top; Esc closes the surface on top.

**The harm if it breaks.** You type into a surface you cannot see, or press Esc and the thing in front of you stays while something hidden closes.

## Why it is a model

The code decides these three things in separate places, each its own list: the render order (`main.rs`, render: the composer inside the screen box, then the window's overlay children, later on top), the key routing (the capture-phase chain: `lang_picker_key`, `help_capture_key`, `settings_key`, `close_popups` on Esc; then the focused pane; the composer reads in the bubble phase), and the Esc order (`close_popups`). Nothing keeps them in agreement. The model encodes each list as the code has it and asks whether they agree.

## Run it

```bash
verify/check.sh verify/overlay-stack/OverlayStack.tla
verify/check.sh verify/overlay-stack/OverlayStack.tla verify/overlay-stack/OverlayStack-before.cfg
cargo test --locked --manifest-path app/Cargo.toml verify_overlay
```

The first checks the code as it is (`Fixed = TRUE`): all three rules hold and `findings.txt` is empty. The second keeps the code before 2026-11-10 (`Fixed = FALSE`) and must still read `breaks (finding)` for all three. The third runs the replays in `app/src/verify_overlay.rs`, which drive each counterexample on the real window and assert the promise: all three pass, and all three fail with the fix taken out. Read model breaks as paths with the pstack `/statechart` skill, using `view.ts`.

## What it found (2026-11-10), replayed on the real window, and what fixed it

- **RaisedIsOnTop broke:** Ctrl+, then Ctrl+Shift+E raises the composer behind the Settings book.
- **TypingGoesToTheTop and EscClosesTheTop broke:** Ctrl+Shift+D (a pane's design menu) then Ctrl+Shift+E raises the composer under the menu; a letter goes into the hidden composer, and Esc closes it while the menu stays.

- **RaisedIsOnTop also broke by a second path:** Ctrl+, then Ctrl+Shift+D raises a pane's design menu under the book (the pane's `OpenThemeMenu` set the menu and ignored the book). Replayed as `verify_overlay_a_panes_design_menu_never_opens_behind_the_book`.
- **The fix:** a surface a chord raises puts away the ones it would otherwise open beneath. `open_composer` closes the book and the design menu first, as T3 leaves its settings page for a new thread (the composer's draft is kept, as ever); a pane's `OpenThemeMenu` closes the book first. The palette the book hosts in place (Settings › Appearance) is a different path and is untouched.
- **Not done:** the audit's proposal, one explicit stack of all ~30 surfaces that the render, the keys and Esc all read. The three surfaces this model covers now keep their promises without it; the stack remains the shape to reach for when another pair is found to disagree.

## Coverage

| Model | Code |
|---|---|
| `Z` (draw order) | `main.rs` render: `render_composer` inside the screen; `menu_overlay` … `settings_overlay` … `help_overlay` in the root's children |
| `TypeLetter` | capture chain (`settings_key`), `on_key` → `composer_key` |
| `PressEsc` | capture chain, `close_popups` (composer, then settings, then theme menu) |
| `Raise` | pane chords: `pane::ToggleSettings`, `OpenComposer` (`open_composer`, which puts the book and the menu away), Ctrl+Shift+D (the pane's DESIGN menu: `wire_pane`'s `OpenThemeMenu`, which puts the book away) |

**Sync commit:** the commit that last changed this skill (`git log -1 --format=%h -- verify/overlay-stack/SKILL.md`). Run `git diff <that commit> -- app/src/main.rs app/src/entry_view.rs app/src/settings_view.rs` and re-read the render children order and `close_popups` before trusting a result.

**Left out:** the other ~25 overlays, the keys sheet (it is drawn above all three and takes its keys first, so it keeps every promise), the mouse.

## When you change the code on purpose

Update `Z`, `TypeLetter`'s or `PressEsc`'s order, or what `Raise` puts away, to match the code. A new surface a chord can raise over these three joins `Surfaces` with its place in `Z`, and gets a replay.
