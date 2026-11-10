---
name: verify-keyboard
description: "Check that every Terminal Delight surface with a text field keeps a person's typing from the shell behind it, on the real window. Use before and after adding or changing a surface that takes typing (a picker, a search, a dialog), changing the capture-phase key chain in app/src/main.rs, or overlay_owns_keyboard; or when a letter typed into a surface shows up in a terminal."
---

# Verify keyboard ownership

**The promise.** A letter typed into a surface that shows a text field goes to that field and never to the shell behind it.

**The harm if it breaks.** A person's words land in a terminal they were not looking at: a shell prompt, or an agent's prompt read as an instruction.

## Why it runs on the real code

Whether a key leaks depends on how the surface was opened. Opened from a focused pane, the pane keeps the focus, and a surface that reads its keys in the bubble phase never hears them. A hand model of gpui's focus would not be true to the code, so the check drives the real window instead.

## Run it

```bash
cargo test --locked --manifest-path app/Cargo.toml -- verify_keyboard
```

`app/src/verify_keyboard.rs`: each case opens a surface the way a person does, from a focused pane running `cat`, proves the shell takes typing first, types `qzx`, and checks both ends. A failure names the surface and the end that broke ("its field holds …", or "reached the shell").

## Coverage (the case table is the record)

Settings search (Ctrl+,), the keys sheet's filter (F1), the language picker from the keys sheet's 🌐 (Settings › General now picks a language with native chips, no picker), the composer (Ctrl+Shift+E), find across panes (Ctrl+Shift+F), the agent launcher (Alt+A), project lookup.

**Sync commit:** `62f87a5b`. A new surface that takes typing is a new row in `CASES`.

## What it found

On its first run (2026-11-10) the language picker leaked from both openers: its keys were read in the bubble phase of `on_key`, after the focused pane. Fixed in the same change by moving them, unchanged, into `lang_picker_key` on the capture chain. Earlier, the same class in the Settings search (2026-11-09), fixed the same way.

## It can fail

Its first run failed on the picker; with the Settings capture call removed, the Settings case fails. A case that never fails is a case that is not opening the surface the way a person does.

## When you add a surface

Add a `Case`: a name, an opener that a person would use (a keystroke or a click on its real selector), and the surface's own text field. If it fails with "reached the shell", read its keys in the capture phase, as `settings_key` and `lang_picker_key` do.
