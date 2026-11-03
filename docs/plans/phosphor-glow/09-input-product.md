# Wrapped Input Editing

Parker's screenshot selects input spanning a wrap, and Backspace should erase that entire selection. Existing selected-word deletion works only on one grid row. A wrapped word, a large wrapped passage and selected multiline input containing the caret should all erase as a single edit. A selection entirely in unrelated history stays a copyable selection.

New shortcuts apply to the focused editable input: Shift+Backspace deletes the previous word; Ctrl+Shift+Backspace deletes the complete current logical line, including its soft wraps; Ctrl+Shift+Alt+Backspace clears the entire editable input. A workbench composer supports the same keys through its existing local editor. Clearing input does not submit it, clear terminal history or send an interrupt. Other visible text fields retain keyboard ownership.

Erased letters use their original text colour, even when selected at deletion, and fade for at least 650ms. The erasure fade falls linearly to zero so its later portion remains visible; ordinary replacements keep the existing 220ms default and curve. The existing zero-off setting wins. Help lists the three shortcuts in every supported language. The native test window remains the delivery target.
