# Architecture

Reuse the existing per-pane glyph history. A decrease in any painted row's nonblank grapheme count is a visual erasure, so all changed old glyphs in that update use at least 450ms. This is a pixel-history rule; it does not infer an editor command or inspect keyboard input. Current text remains immediate.

Remove bottom-alignment offset from history invalidation: snapshots already contain painted rows, so their old coordinates are exactly where an afterimage belongs. Preserve invalidation for real viewport scroll, dimensions, theme/focus, alternate screen, inversion, hidden tabs and covered faces. Bound history by the larger of 4,096 glyphs and one viewport's cells so a full visible passage fits; repeated updates retire the oldest ghosts within that bound.

A transient TerminalLightPreset travels on the resolved Theme, initialized from its design before palette overlays. This avoids using Theme.name after a palette has renamed it. Grade's optional fields remain optional; drawing and GAUGES use the same preset-aware edge accessors. No new saved field, config endpoint or GPU patch is required.

Update house_outer's fresh picture grade to the shared screenshot values. Keep Grade::default as the compatibility fallback for fields omitted from older grade files: an absent historical phosphor still means the original unit gain, and an absent bench size still follows text size. Keep neutral reset's existing semantics and saved explicit values intact. Update the existing test build only, preserving its hosted shells and capturing the selected new defaults in separate native fixtures.
