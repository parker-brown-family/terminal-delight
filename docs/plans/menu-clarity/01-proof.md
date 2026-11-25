# What changed in the menus

At 600 pixels, the Floor now puts the target below the actor and verb. The target still accepts a click. On the Settings Appearance page the terminal samples move to a new line before the setting’s name gets squeezed. At 1600 pixels the controls share their original row.

The pane context menu now shows Ctrl+Shift+C, Ctrl+Shift+V and Ctrl+Shift+K beside their actions. Copy appears when the terminal or the workbench has a selection. The pane and Floor both spell out the current access setting in TERMINIMALIST, using the existing translation for “selected” in all nine languages.

| Test | Deliberate break | Observation |
| --- | --- | --- |
| Floor relationship at 600 pixels | Keep the wide layout down to one pixel. | The visible target no longer fits beneath the verb. |
| Settings titles at 600 and 1600 pixels | Remove wrapping and allow the text column to shrink to zero. | The title falls below its 112-pixel minimum. |
| Clipboard menu before and after selecting text | Remove the Paste chord. | The window cannot find a visible Ctrl+Shift+V hint. |
| Access choices in nine languages | Mark the unselected choice in the text skin. | The selected label differs from the localized expected value. |

The four cases passed with the repairs and each failed with its corresponding change reversed. The pictures are native windows. Open the [audit](00-audit.md) for the baseline and the [after photographs](screens/after/) for the same views.

At 600 pixels the broken target measured 2 pixels wide. The program now gives an empty target a 148-pixel card on its own row. The old palette label measured 31.5 pixels and wrapped to 256 pixels high. Its new row reserves at least 112 pixels for words. The first after photograph also showed how faint the shortcut text was. The final menu uses eighty percent of the action’s foreground colour for the key.

| Compare the 600-pixel windows | Before | After |
| --- | --- | --- |
| Floor line in the text skin | [Open](screens/before/terminimalist-600-floor-line.webp) | [Open](screens/after/terminimalist-600-floor-line.webp) |
| Appearance in the default skin | [Open](screens/before/default-600-settings-appearance.webp) | [Open](screens/after/default-600-settings-appearance.webp) |
| Pane menu in the default skin | [Open](screens/before/default-600-pane.webp) | [Open](screens/after/default-600-pane.webp) |
| Selected access in the text skin | [Open](screens/before/terminimalist-600-pane.webp) | [Open](screens/after/terminimalist-600-pane.webp) |
