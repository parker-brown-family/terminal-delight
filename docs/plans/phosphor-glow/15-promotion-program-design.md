# Program design — regression and promotion checks

Extend the existing GPUI pane harness instead of introducing another editor simulator. Enumerate embedded skins and designs from their registries, exercise actual retained grids with glass on/off, focus changes, wrapped deletion and selected/cursor styling, and verify history expiry and explicit zero. Add palette-overlay and saved-pin coverage using existing theme/grade APIs. Demonstrate that intentional effect-disable mutations fail the new behavioral checks.

Use a temporary GPU-query probe around the native command encoder and a separate CPU layout/paint span. Query support is observed explicitly; unavailable timestamps are not zero. Preserve the probe diff, exact compiler inputs, workload/case receipts, sample counts and p50/p95 results. Include one/several-pane, replacement, erasure and scroll cases at flat/warped settings, plus bounded-history and expiry behavior. Select the promotion frame budget before examining results.

Run both complete core suites and strict lints on the integrated tree, replay the GPUI patch stack from its pin, and run native representative skin/style checks. Remote CI and the branch's merge policy must pass before integration. Rebuild the actual integrated snapshot before installing; retain rollback to the previously running binary.

## Budget selected before sampling

The native desktop survey on 2026-10-21 reports eDP-1 at 2560×1600 physical pixels and 60.008Hz (scale 1.6), a 16.664ms refresh period; the external monitor is also 60Hz. For this machine's tested workloads, require GPU command p95 below half that period (8.332ms), and CPU layout/paint p95 below one period (16.664ms). Report both distributions separately and compare disabled/enabled cases. These are promotion criteria for this measured configuration, not input latency promises or qualification of the untested 165Hz mode/other adapters. If a workload fails, fix the measured cost or retain an explicit qualification gap before claiming it passed.
