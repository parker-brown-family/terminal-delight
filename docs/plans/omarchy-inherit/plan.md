# Plan: Inherit Omarchy colours

Difficulty 5/10. One combined approval before implementation.

## What it is for
Add INHERIT beside INVERT in the outer theme tray. Outer follows the selected Omarchy palette live; panes following outer receive it. The existing pass theme down button attaches other panes.

## Shape
- Outer-only INHERIT, off by default. It is a sticky saved setting: continuously watch across successive Omarchy theme changes and resume watching after TD restarts. Enabled state says watching and shows the selected theme name.
- Keep manual colour settings while following. Off restores them. A palette, colour-set or colour-pip gesture exits inheritance and applies the edit. Style, skin, syntax, INVERT and display gauges remain usable.
- Suspend saved manual palette/dynamic/seed/text/complement/human colour modifiers while following. Apply Omarchy colours before INVERT and display grades.
- Reuse palette::from_source and theme::overlay_palette. Load active colours on the background executor at the existing 300 ms reload cadence, never during painting. Compare contents or a reliable replacement signature to catch replacements preserving timestamps. Bump theme generation and refresh outer and panes together.
- Installed source: ~/.local/state/omarchy/current/theme/colors.toml, with theme.name beside it. Support legacy ~/.config/omarchy/current only when the primary tree is absent; a malformed primary must not silently choose another source.
- Missing/malformed source shows unavailable and holds last valid colours. If none has loaded, render manual colours and show unavailable. Persist the intent separately from source availability.
- Store source selection in ThemeChoice/ThemeGroup; existing PaneTheme::effective and pass_theme_down propagate it. Existing independent panes keep their settings until pass down. Pane menus continue using follow outer.
- Files: app/src/palette.rs (active palette state/load), app/src/theme.rs (source selection, resolution/reload), app/src/main.rs (outer toggle and manual edit exit); language table if surrounding labels require translation.

## Least confident decisions
1. Manual colour edits leave inheritance. INVERT and display/style changes keep following.
2. Unavailable source holds the last good colours with visible status.

## Slices and proof
1. Tracer bullet: toggle loads actual staged Omarchy palette; outer and following pane update. Off restores manual colours. Independently painted pane stays independent.
2. Reload/persistence: directory replacement with preserved timestamps, malformed then valid input, absent Omarchy, restart enabled, INVERT and pass down. Run focused theme/palette/pane behaviour tests and required repository checks. Inspect tray at its actual width.

## Evidence
palette.rs::from_source translates Omarchy format. theme.rs::init polls paths every 300 ms and invalidates theme generation. PaneTheme::effective and Workspace::pass_theme_down already implement live pane inheritance. Installed omarchy-theme-set stages then renames current/theme and writes theme.name. These are source observations; new behaviour is proposed and unbuilt.
