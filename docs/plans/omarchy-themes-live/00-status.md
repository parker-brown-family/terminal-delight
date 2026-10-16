# Omarchy themes, live — status

**Difficulty: 4/10** (scored 4, raised to 5 for a frost-shader change, back to 4 when Parker pointed at the outer's paint card: the glass already existed, so no renderer work). Everything is cheap to undo: the colour-set code stays for saved sessions, and a shelf comes back with one line.

| Gate | State |
|---|---|
| Combined plan, first draft (`01-plan.md`) | 2026-10-16, amended by Parker's notes rather than approved |
| Combined plan, second draft (`reports/2026-10-16-omarchy-themes-live.html`) | 2026-10-16, **approved** ("Let's go!") with three answers, below |
| Glyph map (`02-glyph-map.md`) | superseded: colour emoji became monochrome icons inked in each theme's accent (`app/src/glyph.rs`) |
| Build | in progress, worktree `.worktrees/omarchy-themes-live`, branch `feat/omarchy-themes-live` |

Parker's answers on the second draft, 2026-10-16:

> hey what? --- ok 1. the outter already got a glass tray! the inner guys need the same treatment! Just use what the OUTTER THEME PICKER ALREADY DOES! Frost the panel is I think what this goes to. 2. agree. 3. IUN general with emojis --- the COLOUR OF THE THEME should be DIRECTLY evident! so an example... liftoff and tokyo liftoff -- both as rockets make sense, but the COLOURS of the rockes is different... Is it cheaper for you to get these from chatgpt in browser or gen yourself? Let's go!

1. Glass: the panes' paint grids now sit on the outer's card, one builder (`paint::card`) for both. No shader change.
2. A theme with no glyph draws its first letter in its accent.
3. Glyphs are Material Design Icons (Apache 2.0) drawn as masks in the theme's own accent on its own background, not colour emoji — so the theme's colour is the first thing the tile shows, and two themes may share a shape. Neither ChatGPT images nor generated art: an inked icon is exact, free per theme, and follows a theme that is retuned.

Asked for, 2026-10-16:

> 1. Can Terminal Delight inherit theme config from Omarchy? I just HID a bunch of themes on Omarchy and I would Love TD to be able to detect this and respond accordingly...
> 2. ok... so TD themes suck! Omarchy themes are GOLD... a) ship TD with the DEFAULT OMARCHY themes as default TD b) Do away with the COLOUR SETS painter
>
> If possible - OVERRIDE the themes picker with the LIVE AND ACTUAL config from OMarchy!

Parker's notes on the first draft, 2026-10-16:

> Theme picker should float on a GLASS element!
>
> Gamba etc. tactical are not themes - they are designs... The designs, skins, etc. all stay -- this is stictly for the colour pickers.
>
> The colour picker section uses glyph style emojis... What I think is that we do the hard work of picking the 30 most popular PUBLISHED omarchy themes and MAKE glyph emojis for themand map them for quick reference here...
>
> https://github.com/tcballard/omarchy-theme-tokyo-liftoff Our own PRAUS (a lantern) top work from hancore should be exceptions to the already published or other rules

Post-hoc difficulty: not yet scored.
