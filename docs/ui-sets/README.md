# Author a Terminal Delight UI set

A UI set decides what chrome a window draws. It is a TOML file of switches, and it sits beside the three looks Terminal Delight already separates:

- **Palette** decides what colour everything is.
- **Design** decides how the glass is finished.
- **Skin** decides the chrome's shape.
- **UI set** decides which pieces of chrome are there at all.

New windows open in Clean with the `clean` set. Other designs default to `standard`: the artifact, palette and slider glyphs remain visible, while counts, scale, split buttons and the remaining menu glyphs are hidden. The `all` set still shows every control.

```sh
terminal-delight ui --list
terminal-delight ui --template all > ~/.config/terminal-delight/ui/mine.toml
terminal-delight ui --check ~/.config/terminal-delight/ui/mine.toml
```

The `all` template names every switch there is, each set to `true`, with a comment saying what it is. Copy it, give it a `name` and an `icon`, and turn off what you do not want. A key you delete means no opinion, and no opinion resolves to shown. A set can be one line long.

`--check` prints every switch as the set resolves it. It exits non-zero on a key no switch answers to, so a typo cannot pass silently. A set written for a newer build still loads; the unknown key is reported and ignored.

Sets in `~/.config/terminal-delight/ui/` appear in UI CONFIG beside `all`, `clean` and `standard`, under the name the file gives.

## Two layers

`[outer]` is the frame around the panes, once per window: the app mark, the Git notice, the agent counter, the left bar and its header, the allowance rails, the attention pill (`spine`), the split buttons, the tab and pane counts, the scale, the six corner glyphs, and `option_rows`.

`[panes]` is **one config for every pane**: the logo, the mode, the DOC | TERM | BENCH toggle (`face`), + AGENT, the handover chip, the pane's surface and sliders glyphs, close, and sticky notes. No pane is in charge of its own chrome.

`option_rows = false` folds the painter palette's rows to their selection. A 750 ms hover opens a row, and so does a click anywhere on it.

## Who decides

1. A switch flipped by hand in UI CONFIG.
2. The set: one picked by hand, or else the one the design asks for (`ui = "clean"` in a theme file), or else `standard`. An unavailable set falls back to `all`.
3. Nobody: shown.

A design only ever asks. Parker, 2026-10-22: *"clean mode shouldn't BLOCK elements from being made visible."* Picking a set clears the hand flips, because picking a set is asking for that set whole.

## What no set can hide

- CONFIG
- each pane's title, its state, and ⋯
- the ◀ / ▶ handle that trades the left bar for tabs across the top

These are the way back and the news, so they are not keys.

A hidden element never hides its news:

- The bug report's dot moves onto CONFIG.
- The Git notice's alert ink moves onto CONFIG.
- A card waiting on the bench shows in the pane's state word when the face toggle is gone.
- Everything a set puts away is still one row away, in CONFIG or in a pane's ⋯.

## New windows

UI CONFIG's NEW WINDOWS row starts a new window as Clean. Its "use this window" button writes `~/.config/terminal-delight/new-window.toml`, which holds this window's design, grade, palette, pinned skin and UI choice. A new window then opens wearing all of it, until the template is deleted.
