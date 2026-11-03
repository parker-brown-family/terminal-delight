# Program Design

## Interfaces

```rust
enum TerminalLightPreset { Standard, Intrusive }
// Theme: transient design identity, retained through palette overlays and CRT off.
terminal_light: TerminalLightPreset

fn image_glow_at_edge(&self, preset: TerminalLightPreset) -> f32;
fn inactive_text_at_edge(&self, preset: TerminalLightPreset) -> f32;
fn letter_glow_for_focus(&self, focused: bool, preset: TerminalLightPreset) -> f32;
fn text_for_focus(&self, focused: bool, preset: TerminalLightPreset) -> f32;

// Trail: observes screen positions, not the terminal's input editing operations.
fn observe(previous, current, now, milliseconds, glyph_budget);
```

The old-glyph lifetime is max(configured fade, 450ms) when any painted row loses nonblank graphemes, otherwise the configured fade. Existing zero clearing takes precedence. Cursor/style-only changes emit no new trail and do not reset its clock. A viewport-cell budget keeps a whole freshly erased viewport, evicting older history first.

## Verification

Test multi-letter erasure, multi-row paragraph removal, shrinkage of bottom-aligned content, movement of the cursor after deletion, expiry at 450ms versus ordinary 220ms, zero disable, whole-viewport capacity, independent panes and real context invalidation. Test Standard/Intrusive values, explicit pins, palette relabeling, CRT off and raw optional-field serialization. Update intentional initial-picture assertions and retain neutral-reset assertions.

Native proof will drive a private terminal program that removes a whole paragraph and then moves the cursor. Timed captures after 220ms and after 450ms distinguish the two lifetimes; the untouched neighbor must remain identical. Build both cores, rerun both lint gates and refresh only the named testing GUI. The exact erasure classification and history bounds are the least-confident choices; the native reproduction judges them before delivery.

The larger initial text size requires a 720px-tall window in app/src/pane/harness.rs so its fixed 100×28 startup grid remains inside the viewport while the test clock holds the native resize debounce. Keep the fixture's deliberate column count: applying the pending full resize reflows the manually authored wrap/link cases and changes what they test. All pointer assertions remain intact. The independent-size test explicitly starts with an unsplit bench, since the new fresh window deliberately starts at a separate 76% bench size.
