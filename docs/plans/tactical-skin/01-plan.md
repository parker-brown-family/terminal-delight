# OPS skin — the plan

**Difficulty: 6/10**, and it holds. Your notes on the workshop changed four things: the type, the name, a new Intrusive design, and double-row gauges. That is a gate doing its job. This page is the one approval the score buys. After it, code.

## What you decided (workshop notes, 19 October 2026)

| question | decision | your words |
|---|---|---|
| Heading face | **F, Squared ops**: Oxanium Bold for headings, Chakra Petch for the interface, Share Tech Mono for numbers | "Squared ops is going ot win here..." |
| Skin name | **OPS** (`ops`). The Tactical design names it with `skin = "ops"` | "the TACTICAL skin will now be called OPS" |
| Intrusive | **A new design in STYLE → DESIGN**, separate from OPS, so it can be mixed with any skin | "That intrusive is wicked... can we get that to be a preset in teh syles? … INTRUSIVE style will be SO nice to mix in!" |
| Gauges | **Two rows of pips per provider**: weekly on top, five-hour under it. The slot already draws both as rails | "they are all DOUBLE bars (mostly) 5 hour + weekly..." |
| Grid font | Unchanged, JetBrains Mono | concur |
| Tactical's dials | The quieter set: glow 0.35, bloom 0.25, bezel 0.45, vignette 0.24, scanlines 0.022, flicker 0.03, jiggle 0, tracking 0.06 | concur |
| The nine marks | All nine, built as skin keys any skin can use | concur |

**One thing I decided for you.** I had recommended IBM Plex Mono for numbers, because Share Tech Mono's zero and capital O are close at 11 px. You picked direction F as a whole, so Share Tech Mono it is. The native captures in slice 7 check the zero against the O on the gauges. If they collide, the fallback is Plex Mono for numbers only, and I'll show you both.

**One thing left out of scope.** Figure 11's orange rows are the surfaces that draw their own colours. They stay out of this pass, which makes them follow-up work for every skin at once, filed as an issue. The pass covers what the skin system reaches today plus what Chalk already patched.

## What the code says, and what it costs

- **Fonts.** `[font]` in a skin takes one family, applied as the UI font (`Skin.font_family`, skin.rs ~688). OPS needs three. The plan is two optional keys, `heading` and `data`, each falling back to `family`, and two helpers so a surface asks for the right one. Fonts are bundled the way Caveat is (`include_bytes!` in main.rs ~43605), with their OFL texts. Oxanium and Chakra Petch have no reserved names. Share Tech Mono reserves "Share", so it ships unmodified.
- **Oxanium is variable only.** Caveat proves gpui loads a variable font. Whether it honours weight 700 on a variable face is **unconfirmed**. If it doesn't, I generate a static Bold instance with fontTools, which OFL permits because there is no reserved name.
- **Cut corners have no gpui property** (issue 407). Paper already paints its rim as a canvas path through `Skin::finish` (paper.rs `rim`). `corner = cut` uses that route: no radius, the panel fill and rim painted as one chamfered path. The shadow stays rectangular. That is a known compromise; drop the shadow under `cut` if it reads wrong.
- **The double gauges exist.** `render_bar_slot` (main.rs ~22481) draws the weekly window as a solid rail and the five-hour window as a notched rail. `gauge = pips` draws both as pips, with hollow dashed pips when a window is unknown. It never draws an empty bar, because unknown is not zero.
- **Intrusive is mostly a design file.** The CRT glass (crt.rs `glass`) is painted over the grid, so heavy scanlines and an accent tint already lie on the text. **Unconfirmed:** whether the grid can bleed glow off every letter (the "wicked" part). If it can't, Intrusive ships without the letter glow first, and the glow becomes its own slice.

## Slices, tracer first

1. **Tracer: OPS exists end to end.** `app/skins/ops.toml` using only keys that exist (square corners, bracket emphasis, tracked caps, flat shine, glow metrics). Three bundled fonts and the `heading`/`data` keys. The Tactical design sets `skin = "ops"`, and OPS is in the picker. Headings and data applied on three surfaces: the top-band notice, the left tree's project rows, and the pane header. Native capture.
2. **Cut corners.** `corner = cut`, through `finish`, `panel`, `bar`, `chip` and the menu-recipe surfaces.
3. **Pips.** `gauge = pips` on the allowance slot's two rails and the GAUGES tray readouts.
4. **The other marks.** Chevron titles, the hatched focus edge, the Pip-Boy selection bar with an arrow tip (`active_row`), glowing counters, and a `grid` material drawn under the text the way paper is.
5. **Intrusive.** `app/themes/intrusive.toml`, registered in `BUILTIN_THEMES`, with picker label and help line in every language, and the built-in count test updated. Letter glow if the grid supports it.
6. **Quieter Tactical.** New dials in `tactical-overdrive.toml`.
7. **Photograph and review.** Every surface in `docs/spec/skin-surface-inventory.md` under OPS, in Tactical's cyan and two Omarchy palettes, flat and warped, plus Intrusive mixed with OPS and with Chalk. Then an annotatable review page, gates (fmt, clippy `-D warnings`, the full suite with `--no-fail-fast`), install, one pull request.

## Done when

- Picking the Tactical design brings OPS, and OPS can still be picked under any design.
- Headings are in Oxanium, interface text in Chakra Petch, and numbers in Share Tech Mono (or Plex Mono, per slice 7). The terminal grid is unchanged.
- Every skin-reached surface in the inventory is photographed under OPS in three palettes.
- Intrusive is offered in STYLE → DESIGN and looks like the left panel of workshop figure 9.
- A saved Field, Chalk or Paper choice still reads back as it does today.
