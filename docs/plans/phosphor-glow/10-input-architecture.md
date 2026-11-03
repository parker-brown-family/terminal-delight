# Architecture

Keyboard selection steps also follow soft-wrap seams; hard row boundaries remain boundaries. The native check exposed the old Shift+Arrow row clamp after mouse-range deletion was fixed. A matching GPUI test now covers selection across every wrap before deletion.

Extend the current arrow-and-DEL selection editor. A pane/input_edit child module finds the soft-wrap chain containing the caret and flattens it into terminal cells. Selections across explicit newlines are permitted only when their row span includes the caret; represent the newline as one editable character. Retain the display-offset and non-agent alternate-screen guards, and reject rectangular selections spanning rows. Reuse erase_run_keys for caret motion, wide-cell skipping and trailing blank handling.

The new deletion chords belong to terminal input after higher layers have taken their keys. Exempt Backspace from the window's blanket Ctrl+Alt claim so the complete-input chord can reach the active editor. The terminal sends standard backward-word deletion, a computed current-line erasure, or beginning-of-input/kill-to-end for a Readline-compatible input area. The latter is verified against a real Bash buffer containing literal newlines. No PTY writes originate outside the person's key event.

Extend workbench::Edit and line_edit so the composer owns these same gestures locally and keeps undo. Do not route them past a document, note or rename field into a hidden shell.

Build the normal text and clean afterglow styles together in the existing styled viewport cache. The clean styles preserve program colours and inversion while excluding TD selection and cursor overlays. Apply the same row-placement transform to both snapshots. GridPaint observes the clean snapshot and draws its existing live one unchanged. No GPU patch, new rendering pass or history texture is needed.
