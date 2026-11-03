# Program Design

## Interfaces

```rust
// Grade: saved optional fields, independently pinnable.
inactive_image_glow: Option<f32>, // 0..1
inactive_text: Option<f32>,       // 0..1 times the active text grade
afterglow_ms: Option<f32>,        // 0..1200 ms; zero disables

fn letter_glow_for_focus(&self, focused: bool) -> f32;
fn inactive_text_at_edge(&self) -> f32;
fn afterglow_ms_at_edge(&self) -> f32;
```

GridPaint receives cell dimensions, a reset context and the effective duration with each snapshot. Its bounded afterglow model compares graphemes at terminal columns, records only replaced nonblank glyphs and exposes fading glyphs at the executor's current time. A geometry/context change seeds a fresh baseline without trails.

The new MCP percentages are inactive_image_glow and inactive_text; afterglow_ms is explicitly milliseconds. Existing image_glow remains the active strength. A get reports absent values as null, with effective defaults documented. A set touches only fields provided.

The GAUGES renderer draws sections from the existing slider helper and translated labels. Percentage readouts cover both glow strengths and the inactive text ratio; afterglow gets a millisecond readout. Reset restores light defaults along with the main gauges, leaving CRT glass/crawl values intact.

## Verification

Cover unset versus explicit zero, default-grade omission, channel pinning, inactive multiplier boundaries and focus rendering without saved-state mutation. Cover changed versus unchanged glyphs, immediate new text, deterministic fade/expiry, zero duration, wide and combining characters, bounded history and context reset. Exercise real focus events through the existing pane harness.

Build a new named native binary. Capture active/inactive settings and a timed replacement sequence in an isolated fixture, then refresh the dedicated testing instance while preserving its live shells. Check both terminal cores and the same PR; normal TD remains available.
