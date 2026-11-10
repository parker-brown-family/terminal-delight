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
cargo test --locked --manifest-path app/Cargo.toml -- --ignored verify_overlay
```

The second runs the replays: `app/src/verify_overlay.rs` drives each counterexample on the real window. They are `#[ignore]`d while the finding stands, and each asserts the promise, so they fail today. Read model breaks as paths with the pstack `/statechart` skill, using `view.ts`.

## What it found (2026-11-10), both replayed on the real window

- **RaisedIsOnTop breaks:** Ctrl+, then Ctrl+Shift+E raises the composer behind the Settings book.
- **TypingGoesToTheTop and EscClosesTheTop break:** Ctrl+Shift+D (a pane's design menu) then Ctrl+Shift+E raises the composer under the menu; a letter goes into the hidden composer, and Esc closes it while the menu stays.

The fix the audit proposes: one explicit stack of surfaces in the window, ordered by when each was raised, that the render, the key routing and Esc all read.

## Coverage

| Model | Code |
|---|---|
| `Z` (draw order) | `main.rs` render: `render_composer` inside the screen; `menu_overlay` … `settings_overlay` … `help_overlay` in the root's children |
| `TypeLetter` | capture chain (`settings_key`), `on_key` → `composer_key` |
| `PressEsc` | capture chain, `close_popups` (composer, then settings, then theme menu) |
| `Raise` | pane chords: `pane::ToggleSettings`, `OpenComposer`, Ctrl+Shift+D (the pane's DESIGN menu) |

**Sync commit:** `62f87a5b`. Run `git diff 62f87a5b -- app/src/main.rs app/src/entry_view.rs app/src/settings_view.rs` and re-read the render children order and `close_popups` before trusting a result.

**Left out:** the other ~25 overlays, the keys sheet (it is drawn above all three and takes its keys first, so it keeps every promise), the mouse.

## When you change the code on purpose

Update `Z`, `TypeLetter`'s or `PressEsc`'s order to match the code. A fix should turn findings into `FIXED?`; then remove them from `findings.txt` and drop the `#[ignore]` on the matching replay test.
