# Fresh install opens flat — combined plan

A fresh install of Terminal Delight opens on a flat, clean screen in the Terminal
Delight palette. The CRT becomes something you find. It sits behind a collapsed
row at the foot of the GAUGES tray, and its settings are already the ones this
machine runs today, so switching it on gives exactly the current look.

## 1. The GAUGES tray

The main list loses **warp** and the three **roll** sliders. Two faint collapsed
rows go at the bottom, in the order you named them:

```
GAUGES — OUTER                      GAUGES — OUTER
text size    ──●──────── 74%        text size    ──●──────── 74%
bench size   ──●──────── 74%        …
brightness   ───●─────── −24        menu bar     ──●──────── 85%
contrast     ──────●──── +10        [ reset ]
colour       ──────────● +50        ▸ text crawl
text         ─────●───── +0         ▾ crt
background   ─────●───── +0           [▣ crt on ]
gamma        ─────●───── +0           warp       ─────────● +150
menu bar     ──●──────── 85%          roll       ─────●────
[ reset ]                             roll spd   ────────●─
▸ text crawl                          roll size  ──●───────
▸ crt                                 ↺ roll → per-theme
```

- Both rows are collapsed every time the tray opens. Nothing is remembered, so
  they stay easter eggs.
- Inside each one, the toggle comes first. The knobs appear only while it is on,
  the way crawl's angle and depth already behave.
- **reset** clears the main gauges only. It no longer flattens warp or clears the
  roll, because those now live in the CRT section, which keeps them for the day
  somebody switches it on.

## 2. What "CRT off" means

`crt` becomes a new grade channel, like `crawl`. It is scoped per pane, follows
outer by default, and is readable and writable over MCP as a boolean.

With it off, the resolved screen has no **warp**, **roll bar**, **scanlines**,
**vignette**, **bloom**, **flicker**, **vertical-hold jiggle**, **glare** or
**bezel**, and the pane skips the **power-on flash**. The values are not touched.
The theme file and the grade still hold them, and switching CRT on restores them.

**Glow stays.** It is the accent halo on the header, cursor and bench cards, and
the chrome skins use it too. It isn't part of the tube.

## 3. The monitor tile in the theme tray

A **📺** tile heads the colour-set column. It paints the Terminal Delight palette,
which is the same Omarchy palette this machine's outer is wearing now
(`palette = "terminal-delight"`). It is lit whenever that palette is worn.

TD will ship its own copy of that palette, so the tile works on a machine
without Omarchy. An installed Omarchy `terminal-delight` theme still shadows the
shipped copy, the same way a user theme shadows a stock one.

Picking any other colour-set tile now clears the palette, which is what the paint
overlay already does. Today the tray lays the set over the palette, so a pane can
wear TD colours with cherry on top, and 📺 and 🍒 would both be lit.

## 4. Fresh-install defaults

| | Shipped today | This machine | Fresh install |
|---|---|---|---|
| Palette | none (amber seed) | terminal-delight | **terminal-delight** |
| Design / skin | custom / default | custom / default | custom / default |
| Program colour | ansi | theme | **theme** |
| Syntax | code | agentic | **agentic** |
| Text size / bench | 75% / follows | 74% / 74% | **74% / follows** |
| Menu bar | 80% | 85% | **85%** |
| Brightness | −12 | −24 | **−24** |
| Contrast | +0 | +10 | **+25** ⚠ |
| Colour | +0 | +50 | **+50** |
| CRT | (always on) | on | **off** |
| Warp (kept for CRT on) | +143 | +150 | **+150** |
| Roll (kept for CRT on) | theme's | 0.52 · 0.81 · 0.21 | **0.52 · 0.81 · 0.21** |
| Crawl | off | off | off |
| Agent theme / anchor | follows outer / bottom | same | same |
| New panes | follow outer | follow outer | follow outer |
| Reader inherits look | off | on | **on** |

⚠ **+25 is my estimate**, and it's the number to argue with. You asked for contrast
"turned up a fair bit". This machine sits at +10 with bloom and glow lifting the
text, and a flat screen loses the bloom. Once it's built I'll photograph a flat
window at +10, +25 and +40 so you can choose by eye.

## 5. Keeping this machine as it is

Every state file written before this build was running the CRT. So a saved grade
with no `crt` key reads as **on**. Only a grade that says `crt = false` is off. A
fresh install writes that line, and yours never will unless you switch it off.

A test loads this machine's saved outer grade and asserts CRT on, warp +150,
roll unchanged. A second test boots from an empty state and asserts the
fresh-install row above.

## Done when

- Empty state: the window opens flat, in TD colours, contrast at the agreed
  value, with both gauge sections collapsed.
- This machine's saved state still looks exactly as it does now.
- Opening **▸ crt** and switching it on gives today's warp, roll, scanlines and
  glare, with no slider moved.
- 📺 is lit on a fresh install, and picking 🍒 unlights it.
- `get_pane_config` reports `crt`, and `set_pane_config` can flip it.
- clippy `-D warnings`, rustfmt, and the theme and grade tests are green.
