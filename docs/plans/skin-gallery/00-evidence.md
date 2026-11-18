# Choose a skin in Appearance

Click a miniature window to apply its skin. The selected name appears above the grid and its tile has a highlighted border and caption. On the Appearance page, Left and Right move through the skins in display order. Up, Down and Tab still move between Settings pages. Typing still searches.

A thumbnail uses the installed skin’s panel, tab, rule, font and terminal field. The sample text uses the window’s current graded terminal palette. A custom file joins the eight built-ins when it is the active skin. Settings uses the same selection and session-save path as the design tray.

## Window proof

The native window test measures all eight tile rectangles at 600 pixels, clicks BLUEPRINT, checks its current marker, moves away by keyboard and moves back. A build with the Left and Right action disconnected stayed on BLUEPRINT and failed. A test-only counter measures sample construction across closed Settings redraws, then proves the counter can advance by opening Appearance. It records zero new samples after closing again.

| Window | Default | TERMINIMALIST | BLUEPRINT selected by keyboard |
| --- | --- | --- | --- |
| 600 px | [Open](screens/default-600-settings-appearance.webp) | [Open](screens/terminimalist-600-settings-appearance.webp) | [Open](screens/blueprint-600-settings-appearance.webp) |
| 1600 px | [Open](screens/default-1600-settings-appearance.webp) | [Open](screens/terminimalist-1600-settings-appearance.webp) | [Open](screens/blueprint-1600-settings-appearance.webp) |

The photographs use the native app on a private Xvfb display. The keyboard captures start with the default skin, send Left twice in the Settings window and read back the active skin before taking the photograph.
