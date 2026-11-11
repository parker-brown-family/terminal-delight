---
name: verify-keys-in-a-pane
description: "Model-check which surface inside one Terminal Delight pane gets a typed letter when a sticky note being written and the header's rename box could both hold a caret, with TLA+ and a pane replay. Use before and after changing keylayer::LADDER or route in app/src/keylayer.rs, or sticky_open, sticky_commit, rename_open, commit_rename or layers_up in app/src/pane.rs, or when a letter is reported landing in the wrong box inside a pane."
---

# Verify keys inside a pane

**The promise.** A pane shows one caret, and a letter goes to the caret the person opened last.

**The harm if it breaks.** A person double-clicks a pane's title to rename it while a sticky note is being written, types the name, and the letters go into the note instead.

## Run it

```bash
verify/check.sh verify/keys-in-a-pane/KeysInAPane.tla
verify/check.sh verify/keys-in-a-pane/KeysInAPane.tla verify/keys-in-a-pane/KeysInAPane-before.cfg
```
```bash
cd app && cargo test --locked --bin terminal-delight the_surface_opened_last_holds_the_caret
```

The first is the code as it stands (`OneCaret = TRUE`): both rules hold and `findings.txt` is empty. The second keeps the code before 2026-11-11 and must still read `breaks (finding)` for both. The replay in `app/src/pane/behaviour.rs` drives a real pane.

## What it found (2026-11-11), and what fixed it

- **Both rules broke.** Path: you press Alt+S and write a note → you double-click the pane's title → you type a letter. The header opened the rename box without looking at the note, and `keylayer::route` puts `Sticky` above `Rename`, so the letter went into the note behind the name's caret. Replayed on a real pane before the fix: the name did not get the letter.
- **The fix:** `TerminalView::rename_open` (now the one door for the header's double-click and right-click) posts a note being written first, as a click away from the note posts it; `sticky_open` commits a name being written first, as a click out of the rename box commits it.
- The owner of a key was already one type: `keylayer::Layer`, picked by `route` from one ordered `LADDER`. What the fifteen booleans in `keylayer::Up` allowed was two caret surfaces up at once, which the ladder then settled by order rather than by what the person opened.

## Coverage

| Model action | Code |
|---|---|
| `YouPressAltS` | `keylayer::pane_chord` (Alt+S) → `TerminalView::sticky_open` |
| `YouOpenTheRenameBox` | the header's double-click and right-click → `TerminalView::rename_open` |
| `YouTypeALetter` | `keylayer::route` over `LADDER` (`Sticky` above `Rename`) |
| `YouFinishTheNote`, `YouFinishTheName` | `sticky_commit`, `commit_rename` (Enter, Escape, a click away) |

**Sync commit:** the commit that last changed this skill (`git log -1 --format=%h -- verify/keys-in-a-pane/SKILL.md`). Check `git diff <that commit> -- app/src/keylayer.rs app/src/pane.rs` for the functions above.

**Left out:** the floating document's and the Document face's note carets (`float_caret`, `doc_caret`, below both on the ladder), the menus (Escape only), the bench, scroll mode, and every key that is not a letter. A next round would add the two document carets as variables.

## It can fail

```bash
verify/mutate.sh verify/keys-in-a-pane/KeysInAPane.tla verify/keys-in-a-pane/KeysInAPane.cfg TypingGoesToTheCaretOpenedLast "s/    \/\\\\ sticky' = IF OneCaret THEN FALSE ELSE sticky/    \/\\\\ sticky' = sticky/"
```

Opening the rename box without putting the note away reads `BREAKS`. In the real code, the replay failed on the code before the fix.
