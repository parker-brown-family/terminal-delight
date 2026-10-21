# Deleted Text

Difficulty: 7/10. This extends the existing experiment under Parker's continuing end-to-end instruction. His 2026-10-21 screenshots approve the current grouped GAUGES and replace the earlier lighting defaults. No separate gate approvals are claimed.

## Behaviour

Ordinary replacements fade for 220ms. When text disappears, the complete visible erased passage stays at its former screen positions and fades for at least 450ms, even if its deletion changes bottom alignment or the cursor moves. Longer fade settings also lengthen erasures; zero disables both. Erased text never enters the PTY, selection, clipboard or terminal model.

Standard starts at 20% active letter glow, 100% inactive glow and 45% inactive text. Intrusive starts at 37%, 100% and 52%. Explicit settings keep their meaning. Both screenshots share text +38, text size 107%, bench size 76%, menu size 100%, brightness -37, contrast +21, colour +20, background -23, gamma +42 and border phosphor 198%. These are the new initial picture defaults, without rewriting existing saved picture grades.

The existing fade-time slider controls normal persistence; its hint states the 450ms erased-text minimum. The approved screenshots themselves are the product mockups; the extra timing is a behaviour change, not a new panel.

## Acceptance

Delete a word and a wrapped paragraph in a real terminal. Every changed old glyph remains at its old screen coordinates beyond 220ms and disappears after the erasure lifetime. Moving the cursor neither clears nor renews that history. Selecting Intrusive changes undeclared lighting defaults without a palette's label or the CRT glass switch changing which preset is used.
