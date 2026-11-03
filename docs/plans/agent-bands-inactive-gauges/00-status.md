# Agent bands and inactive-pane gauges

Difficulty: 6/10. One bug with a mechanism to find, a rendering change that touches every agent pane, and a renderer patch for the blur. Parker supplied the spec on 2026-10-31 ("You use your judgement for the remaining instances if you find a pattern"); this page records the judgement calls in place of a separate approval.

## The pattern

Black slabs in agent panes were one mechanism, not a series of bugs. TERMINIMALIST paints the terminal field ungraded and reports that colour to programs (OSC 11). Claude Code and Codex derive their user-turn, composer and pill shades from it. TD then ran every program-chosen background through the full monitor grade, which cuts lightness to about a seventh, so each shade landed below the field and read as black.

## Decisions

- Where a skin owns the field, a program's own backgrounds are painted ungraded, in the colour space TD reported. TD's own cursor and selection colours stay graded.
- In agent panes, a background that is a shade of the field (grey, or the field moved evenly on every channel, within 0.3) is drawn as lines, not a fill. A run covering nine tenths of a row or more is ruled above and below (the Claude input-box pattern); consecutive ruled rows share one pair of rules; a wrapped turn's short last row joins the rule above it. Any other run of two or more cells gets a hairline box (the sticky-note pattern). Diff reds and greens are not shades and keep their fills.
- Scope: agent panes under a skin that owns the field (TERMINIMALIST today), across every design.
- Inactive brightness: a new gauge, 0..100, default 78 (the 22% veil every inactive pane has always had); the veil is capped at 90%.
- Inactive blur: a new gauge, 0..100, default 0. 100 is 9 logical px, a touch past the 7px popup backdrop. It needs patch 0012-tube-blur: a per-tube blur radius in the spare `image_glow.y` lane, taps clamped inside the tube. Only the terminal area blurs; the header stays sharp.

## Status

- [x] Mechanism traced; colour space fixed; bands drawn as rules and notes.
- [x] Both gauges in grade, looks, MCP, GAUGES tray and nine locales.
- [x] Patch 0012 applies to the canonical stack (prepare-gpui.sh order).
- [x] Native captures on Xvfb: rules, note box, diff fills kept, inactive blur and veil.
- [ ] Gates, CI, merge into dev/0.5.0, cutover.

Post-hoc difficulty: not yet measured.
