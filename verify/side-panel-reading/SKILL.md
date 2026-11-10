---
name: verify-side-panel-reading
description: "Model-check how a side-panel tab (the changed-file list, a file's text, a pull request, its diff, a file's patch, the + page's suggestions) shows its reading: an answer, a red note, reading…, read again on a poll. TLA+ plus the real window's tests. Use before and after changing Reading or Shown in app/src/flyout_view.rs, or changes_read, file_read, pull_read, patch_read, suggest_read, read_file's \"nothing moved\" answer, or when a red note, a stuck spinner or a blanked list is reported in the side panel."
---

# Verify the side panel's reading

**The promise.** A red note is about the latest read, a failure is always shown, and a moment's trouble never blanks what a person was reading.

**The harm if it breaks.** A note says a file is gone while its text sits there, a failure goes unseen, or a list a person was reading vanishes because one read failed.

## Run it

```bash
verify/check.sh verify/side-panel-reading/SidePanelReading.tla
verify/check.sh verify/side-panel-reading/SidePanelReading.tla verify/side-panel-reading/SidePanelReading-before.cfg
```
```bash
cd app && cargo test --locked --bin terminal-delight flyout_view::tests
```

The first checks the code as it is (`OneState = TRUE`); every rule holds and `findings.txt` is empty. The second checks the code before the change (`OneState = FALSE`) and must still read `breaks (finding)` for `ErrorIsTheLatestRead`; it is kept so the finding stays reproducible. The tests are the real code: `a_reading_is_busy_exactly_while_a_read_is_out`, `a_failure_keeps_the_answer_before_it_and_an_answer_ends_the_note`, `nothing_moved_makes_the_answer_current_and_its_old_note_goes`, and the window replay `a_file_back_as_it_was_takes_its_red_note_with_it`. Results read as in `verify/check.sh`. Read any break as a path with the pstack `/statechart` skill, using `view.ts`.

## What it found (2026-11-10), and what fixed it

- **ErrorIsTheLatestRead broke** in the five-field `Reading` (got, error, busy, at, task). Path: the poll reads a file → it lands → the file is moved away → the poll's read fails ("The file is not there any more.") → it is moved back, so its time and size are as they were → the poll finds nothing moved. That arm set only `busy` and `at`, so the red note stayed over text that was there, until the file changed or someone pressed r. Replayed on the real window before the fix: it failed with exactly that note.
- **The fix:** `Reading` is now what is shown, one `Shown` value (never read, an answer, or a failure that keeps the answer before it), and the read that is out (`out`, which is also what busy means). Only a landing sets what is shown: `land` for an answer or a failure, `same` for "nothing moved", which makes the held answer current and drops a failure since it. The brief's drawing had one five-way enum; this keeps the answer and the failure drawn while they are read again, as before, because the five-way enum would blank a red note for the whole of every retry (a GitHub read may take thirty seconds).

## Coverage

| Model action | Code |
|---|---|
| `YouOrThePollRead` | `flyout_kick` (poll each second, forced on open, switch, `r`, save) → `changes_read`, `file_read`, `pull_read`, `patch_read`, `suggest_read`; the guard is `Reading::due` or `force && !busy()`; `Reading::start` |
| `ReadLands` | `Reading::land(Ok)`; `read_file` reads whole only when forced, with nothing held, or when its time or size moved |
| `NothingMoved` | `read_file` → `Ok(None)` → `Reading::same` (files only) |
| `ReadFails` | `Reading::land(Err)` |
| `TheDiffIsDropped` | `pull_read` (`p.diff = Reading::default()` when the pull request moved), `changes_landed` (`patches.retain`), `flyout_kick` (closed tabs); dropping `out` cancels the read |
| `ItGoesAway`, `ItComesBack`, `SomeoneEditsIt` | the file, the repository, or GitHub |

**Sync commit:** the commit that added this skill (`git log -1 --format=%h -- verify/side-panel-reading/SKILL.md`). Run `git diff <that commit> -- app/src/flyout_view.rs` and look for changes to `Reading`, `Shown`, the five `*_read` functions and `read_file`; if any changed, re-check the table, update the model, then note the new commit here.

**Left out:** time (a read is due or it is not), what an answer holds, several readings at once (each is its own), the + page's finder cache (it keys on `at()`), the find in a file. Every action is allowed on every kind of tab, which over-approximates: a path through `NothingMoved` is a File tab's path.

## It can fail

Each rule was broken on purpose in a copy of the model with `verify/mutate.sh`, and each read `BREAKS`:

```bash
verify/mutate.sh verify/side-panel-reading/SidePanelReading.tla verify/side-panel-reading/SidePanelReading.cfg TroubleIsShown "/^ReadFails/,/^\$/s/error' = TRUE/error' = error/"
verify/mutate.sh verify/side-panel-reading/SidePanelReading.tla verify/side-panel-reading/SidePanelReading.cfg NeverBlankedByTrouble "/^ReadFails/,/^\$/s/UNCHANGED <<got, /got' = FALSE \/\\\\ UNCHANGED <</"
verify/mutate.sh verify/side-panel-reading/SidePanelReading.tla verify/side-panel-reading/SidePanelReading.cfg ErrorIsTheLatestRead 's/IF OneState THEN FALSE ELSE error/error/'
```

In the real code, making `Reading::same` keep a `Failed` with an answer (the old behaviour) fails both `nothing_moved_makes_the_answer_current_and_its_old_note_goes` and the window replay, with the note "The file is not there any more."

## When you change the code on purpose

A new way for a reading to land (another "nothing moved", a partial answer) is a new action in the model and a new method on `Reading` that sets `shown`; never set what is shown from outside it. A new kind of tab needs only a row in the coverage table if it reads through `Reading`. If a rule breaks after a change, it is a bug in the change until a replay on the real window says the model is wrong.
