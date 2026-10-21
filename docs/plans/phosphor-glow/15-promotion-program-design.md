# Program design — regression and promotion checks

Extend the existing GPUI pane harness instead of introducing another editor simulator. Enumerate embedded skins and designs from their registries, exercise actual retained grids with glass on/off, focus changes, wrapped deletion and selected/cursor styling, and verify history expiry and explicit zero. Add palette-overlay and saved-pin coverage using existing theme/grade APIs. Demonstrate that intentional effect-disable mutations fail the new behavioral checks.

Use a temporary GPU-query probe around the native command encoder and a separate CPU layout/paint span. Query support is observed explicitly; unavailable timestamps are not zero. Preserve the probe diff, exact compiler inputs, workload/case receipts, sample counts and p50/p95 results. Include one/several-pane, replacement, erasure and scroll cases at flat/warped settings, plus bounded-history and expiry behavior. Select the promotion frame budget before examining results.

Run both complete core suites and strict lints on the integrated tree, replay the GPUI patch stack from its pin, and run native representative skin/style checks. Remote CI and the branch's merge policy must pass before integration. Rebuild the actual integrated snapshot before installing; retain rollback to the previously running binary.

## Budget selected before sampling

The native desktop survey on 2026-10-21 reports eDP-1 at 2560×1600 physical pixels and 60.008Hz (scale 1.6), a 16.664ms refresh period; the external monitor is also 60Hz. For this machine's tested workloads, require GPU command p95 below half that period (8.332ms), and CPU layout/paint p95 below one period (16.664ms). Report both distributions separately and compare disabled/enabled cases. These are promotion criteria for this measured configuration, not input latency promises or qualification of the untested 165Hz mode/other adapters. If a workload fails, fix the measured cost or retain an explicit qualification gap before claiming it passed.

## Measured stress correction

The first actual whole-viewport erasure probe failed both targets: GPU p95 9.861ms and CPU draw p95 134.924ms over 48 frames in a four-second sample, on the NVIDIA RTX 3080 Laptop GPU/Vulkan. The fixture's grid is 151×36 cells and its full passage contains 5,250 letters. `smoke-one/receipt.json` and the raw timestamp/CPU log live under the isolated lab's `native-promotion/` directory. This is a short diagnostic sample, not a complete qualification.

Address the measured layout cost by joining adjacent retained cells into styled row segments. Preserve gaps, wide/combining graphemes, original colours, independently aged fades, layer order and each old line's crawl origin. A 5,400-glyph synthetic viewport now needs 36 layout groups. Keep the retained-glyph budget unchanged. Three new tests cover that bound and the grouping invariants; all 13 trail tests and seven actual grid tests pass before native remeasurement.

Evaluate the existing 16 GPU taps explicitly in the same order, at the same offsets and weights, with unchanged bright-pass, pane bounds and screen blend. Carry this as additive patch 0010 after the immutable image-glow patch 0009 so existing experimental stacks upgrade correctly. Keep the old measured binary and six flat/warped captures for automated before/after pixel comparison. Timing and native image equivalence must be rechecked before claiming qualification.
