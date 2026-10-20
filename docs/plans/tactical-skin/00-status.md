# Tactical skin — status

**Difficulty: 6/10.** The same shape as Chalk: one skin carried across every native surface and kept readable under any palette. On top of that, the skin file gains five keys and an OFL font is bundled for the interface.

## Direction — Parker, 19 October 2026

> "we are going to do a TACTICAL rework folding tactical and field into a single design and adding a skin … kind of an xcom 2 fallout 4 vibe - let's start by workshopping examples of the font + interface!"

His references were XCOM 2's ability panel and objectives list, Fallout 4's Pip-Boy inventory and the FallUI mod, a DIN 1451 construction sheet, a ChatGPT table of "recommended combinations" (DIN 1451 + IBM Plex, Rajdhani + Inter + Plex Mono, Barlow Condensed + Inter Tight, Eurostile Extended + Plex Mono), and the KELOWNA OPS styling on parker-job.brownfamilysports.com. A Fallout comparison titled "INTRUSIVE AND ANNOYING … NOT" sets how heavy the overlay may be.

Field was folded into Tactical the same evening, in [pull request 1029](https://github.com/parker-brown-family/terminal-delight/pull/1029). What remains is Tactical's own skin and type. This continues [issue 990 — Differentiate Design and Skin, with DIN 1451 for Field and Tactical](https://github.com/parker-brown-family/terminal-delight/issues/990).

## Where it stands

- [x] Workshop 1: type and interface specimens — `reports/2026-10-19-tactical-workshop.html`
- [x] Parker's notes on the workshop (19 October): direction F Squared ops; skin renamed OPS; Intrusive becomes a design; gauges are double rows
- [x] One combined plan page — `01-plan.md`
- [ ] Plan approved
- [ ] Native build, photographed on every surface in `docs/spec/skin-surface-inventory.md`
- [ ] Merge, install, review

## Proposed in workshop 1 (not approved)

- Headings and names: D-DIN Condensed Bold / D-DIN Bold (Datto, SIL OFL 1.1). Numbers and readouts: IBM Plex Mono. Terminal grid unchanged (JetBrains Mono).
- The Tactical design names its skin with `skin = "tactical"`, as Deco does. Colours come from the active palette.
- A `tactical` skin with nine marks: cut corners, a hatched strip on the focused pane, chevron titles, a Pip-Boy selection bar with an arrow tip, segmented pip gauges (hollow when unknown), a bracket tab on TERM / BENCH, glowing counters, DIN caps with tracking, and a 44 px grid wash under the text.
- Effects quieter than Tactical's current glow 0.78 and bezel 0.98: glow 0.35, bloom 0.25, bezel 0.45, vignette 0.24, scanlines 0.022, flicker 0.03, jiggle 0, tracking 0.06. Chosen by eye and not measured.

## Workshop sources

The page's embedded fonts were subset with fontTools from google/fonts and the D-DIN release, all OFL. The build kit (subset fonts, `assemble.py`, `measure.py`) is in `~/.cache/td-tactical-workshop/`, outside the repo. Figure 4's widths are the one measured number: advance widths at 14 px, no tracking, no kerning.

## New skin vocabulary the marks need

Bracket tabs (`emphasis = bracket`), tracked caps (`caps = tracked`), glow metrics and the `[font] family` exist already. New: `corner = cut` (issue 407), a `pips` gauge shape, a chevron title shape, a hatched focus edge, a `grid` material, and a second `[font]` family for numbers. See figure 10 of the workshop.
