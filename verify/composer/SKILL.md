---
name: verify-composer
description: "Model-check the composer's faces (the sentence, a chip's list, the add-a-project dialog over it) and what PROJECT's list shows for what is typed on it, with TLA+ and the real window's tests. Use before and after changing Composer, composer_key, composer_escape, refilter, project_rows or the chip and row presses in app/src/entry_view.rs, or add_project_open / add_project_escape / the folder-taken path in app/src/addproject_view.rs, or when a chip's list is reported showing the wrong rows."
---

# Verify the composer

**The promise.** PROJECT's list shows what its field says: every project when nothing is typed, only the matches when something is. And while the add-a-project dialog is up, it is the only face that takes keys.

**The harm if it breaks.** A person opens PROJECT, sees a short list under an empty field, and concludes their project is missing.

## Run it

```bash
verify/check.sh verify/composer/Composer.tla
verify/check.sh verify/composer/Composer.tla verify/composer/Composer-before.cfg
```
```bash
cd app && cargo test --locked --bin terminal-delight composer
```

The first checks the code as it is (`Derived = TRUE`): both rules hold and `findings.txt` is empty. The second checks the code before 2026-11-10 (`Derived = FALSE`) and must still read `breaks (finding)` for `ListMatchesField`. The window replay is `a_chip_left_with_letters_typed_opens_again_on_its_whole_list` in `app/src/addproject_view/tests.rs`. Results read as in `verify/check.sh`; read a break as a path with the pstack `/statechart` skill, using `view.ts`.

## What it found (2026-11-10), and what fixed it

- **ListMatchesField broke.** Path: you open the composer → you Tab to PROJECT → you type letters (the list narrows) → you Tab on (the letters clear) → you Shift+Tab back to PROJECT. The field is empty and the list is still the narrowed one, because PROJECT's rows were a stored list (`Composer::order`) that only typing, Backspace and a new project recomputed; Tab, Esc, Enter, and a press on a chip or a row all cleared the letters and left the rows. Replayed on the real window before the fix.
- **The fix:** the stored list is gone. `Composer::project_order` works the rows out from what is typed each time they are read (`launcher::filter`, a substring match over the projects), so there is no copy to go stale. Last night's brief drew one `Face` enum for the whole composer; the model found that the faces themselves keep their promises and the one break was this copy, so the fix removes the copy and leaves the faces as they are.

## Coverage

| Model action | Code |
|---|---|
| `YouOpenIt` | `entry_view.rs` `open_composer` |
| `YouMoveBetweenFaces` | `composer_key` `tab`, `escape` and `enter` on a chip; `composer_escape`; the chip press (`composer-chip-*`); a project row or choice row press |
| `YouTypeOnAChip`, `YouBackspaceItAway` | `composer_key` typing and `backspace` → `Composer::refilter` |
| `YouPressTheSentence` | the draft's press (`composer-text`) |
| `YouAddAProject`, `YouGoBackFromTheDialog`, `TheFolderIsTaken` | `addproject_view.rs` `add_project_open`, `add_project_escape` (`Step::Machines`), the folder-taken path |
| `YouPutItAway` | `composer_put_away` (Esc on the sentence, Ctrl+Shift+E, a LOOK AT ME row) |

**Sync commit:** the commit that last changed this skill (`git log -1 --format=%h -- verify/composer/SKILL.md`). Run `git diff <that commit> -- app/src/entry_view.rs app/src/addproject_view.rs` and look for the functions above; if any changed, re-check the table and the model before trusting a result.

**Left out:** which projects match (all, or narrowed), the other chips' lists (`Composer::choices` is worked out each time it is read, so it keeps no copy), the sentence's text and pictures, the send, far machines.

## It can fail

```bash
verify/mutate.sh verify/composer/Composer.tla verify/composer/Composer.cfg TheDialogIsTheFace 's/under. = face/under'"'"' = "adding"/'
```

That lets the dialog open over itself, and `TheDialogIsTheFace` reads `BREAKS`. `ListMatchesField` is shown failing by the before config, which is the code that broke it. In the real code, the code before the fix, which stored the rows, failed the window replay with 2 rows where 15 belong.

## When you change the code on purpose

A new way into a chip's list is a new step in `YouMoveBetweenFaces`; a new stored copy of anything a chip's list shows is a new variable in the model, and the first thing to check.
