# Omarchy themes, live

> **Superseded by the second draft**, `reports/2026-10-16-omarchy-themes-live.html`, which folds in Parker's notes on this page: the tray becomes the Omarchy picker with an emoji per theme, on glass. Sections below still hold except where that draft changes them.

## What is on the machine today

Terminal Delight already paints panes in Omarchy's themes. `app/src/palette.rs` reads every `colors.toml` under `/usr/share/omarchy/themes` and `~/.config/omarchy/themes` and puts them on the paint overlay's DESKTOP PALETTES shelf. It reads them once, at startup, and it shows all of them.

Omarchy 4.0.4 has no way to hide a theme. The hiding you did this morning comes from `~/.local/bin/theme-veil`, a script on this machine (not part of Omarchy) that deletes picker preview links for every name listed in `~/.config/omarchy/hidden-themes`. That file lists ten themes today: ristretto, rose-pine, lupine, lumon, solitude, vantablack, white, flexoki-light, catppuccin and catppuccin-latte. So Omarchy's picker shows 18 themes and TD's shelf still shows all 28.

```
                     TODAY                          AFTER
  FAVOURITES         14 tiles (4 are colour sets)   10 tiles
  COLOUR SETS        11 tiles                       (gone)
  DESKTOP PALETTES   28 tiles                       OMARCHY THEMES  18 tiles
                                                    = the 18 in Omarchy's own picker
```

## The change

**1. TD reads the hide list.** It reads the same file `theme-veil` reads, `~/.config/omarchy/hidden-themes`, and parses it the same way: one name per line, `#` comments and whitespace dropped. It does not read the picker's preview cache, because Omarchy deletes and rebuilds that folder every time the picker opens and it does not exist until the picker has run once. The file holds the decision itself.

**2. Changes show up live.** TD already runs a 300 ms loop that watches its own theme file. That loop will also check the modification times of the theme folders and the hide list, and rescan when any of them changes (about 29 small files). Hide a theme in Omarchy and its tile leaves TD within a second, even with the overlay open. Install a theme and its tile appears the same way.

**3. Omarchy's list is the picker.** When Omarchy is installed, the shelf shows Omarchy's themes minus the hidden ones, plus TD's own `terminal-delight` palette (the house look, also hideable). TD will carry the 22 stock Omarchy `colors.toml` files in the binary, alongside the `terminal-delight` palette it already ships, so a machine *without* Omarchy gets the same themes. On an Omarchy machine those copies never appear in the picker. They exist so a pane's colours stay resolvable if the theme it wears disappears. Omarchy is MIT-licensed; the copies carry a credit and the licence text, and a test compares them with `/usr/share/omarchy/themes` wherever that folder exists, so drift fails a test instead of going unnoticed.

**4. Colour sets leave every picker.** The paint overlay loses its COLOUR SETS shelf, which leaves FAVOURITES and a shelf renamed OMARCHY THEMES. The theme tray's column of set glyphs loses its eleven sets and keeps the 📺 house tile and the custom colour wheel. The starter favourites for a fresh install swap their four sets for tokyo-night, gruvbox, nord and kanagawa (distinct first letters from the six palettes already there). In your own `favourites.toml`, the four `set:` lines stop drawing tiles and stay in the file.

## Hiding and removing never take a look off a pane

Your saved session has one pane wearing the colour set `wood` and one wearing `catppuccin`, which you just hid. Both keep exactly what they wear. "Hidden" and "removed" mean *not offered*; they never re-paint anything. The colour-set code stays in place to support this, because a saved session names sets by their key. The next pick you make on either pane replaces the old look the usual way.

## Decisions with my defaults

| Question | Default | The other way |
|---|---|---|
| Does the theme tray's column become a column of Omarchy tiles too? | No. It only loses the sets, and the paint overlay is the Omarchy picker | Replace the set glyphs with small Omarchy screen tiles |
| A hidden theme you starred as a favourite | Hidden there too, and the star stays in the file | Favourites ignore the hide list |
| Did "TD themes suck" include the six built-in textures (hacker, deco, gamba, quiet-command, field-command, tactical-overdrive)? | No. They carry CRT effects and fonts, not colour schemes, and stay | Remove or demote them as well |

## How it gets checked

- Unit tests: hide-list parsing (comments, blanks, a `/` in a name), the picker filter, the rule that shipped copies stay out of the picker while Omarchy is present, that a hidden or retired look still resolves on a pane wearing it, that the shelf key `z` still cycles two shelves, and the shipped-versus-installed drift test.
- The CI gates: `cargo clippy -D warnings` and `rustfmt --check`.
- One photographed overlay, from a hidden test window, showing the 18 tiles. Then hide one more theme and photograph again to show 17.
- Built in its own worktree off `main` and landed as one pull request.
