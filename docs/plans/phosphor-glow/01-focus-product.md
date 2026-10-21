# Terminal Light

Difficulty: 7/10. The focus-dependent text treatment, fading text and grouped controls form the second phase of the same effect branch. Parker asked to get all of them in on 2026-10-21. His earlier instruction to finish end to end remains the execution authority; these design choices are recorded before implementation, without claiming separate gate reviews.

## Behaviour

The selected terminal starts with 15% letter glow. Other terminal panes start with 65% glow and 75% of the selected text intensity; the latter is a starting assumption offered to Parker for adjustment. A pane's existing text grade stays its baseline. Focus changes its drawing, never its saved settings.

New text appears immediately. Replaced letters remain briefly and fade according to an afterglow slider, initially 180 milliseconds, adjustable from zero to 1,200 milliseconds. Zero clears the trails immediately. Terminal text light is independent of the CRT glass switch, so these controls work on flat terminals too.

GAUGES has visible Type, Picture, Active terminals, Inactive terminals and Afterglow sections. CRT glass and Text crawl keep their expandable controls. Existing picture values are preserved. Letter glow is the name on screen; the existing image_glow wire key remains stable.

## Acceptance

Switch focus between two otherwise identical panes: the 15% and 65% drawing treatments must exchange immediately without changing their saved grades. Replace a stationary line: new letters appear on the first update and old letters then fade to nothing. A neighbouring pane and document/workbench surfaces must never acquire those old letters.

The mockup is in mockups/terminal-light.html. All defaults and timing remain adjustable in the real native testing build.
