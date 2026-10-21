# Program design — regression and promotion checks

Extend the existing GPUI pane harness instead of introducing another editor simulator. Enumerate embedded skins and designs from their registries, exercise actual retained grids with glass on/off, focus changes, wrapped deletion and selected/cursor styling, and verify history expiry and explicit zero. Add palette-overlay and saved-pin coverage using existing theme/grade APIs. Demonstrate that intentional effect-disable mutations fail the new behavioral checks.

Use a temporary GPU-query probe around the native command encoder and a separate CPU layout/paint span. Query support is observed explicitly; unavailable timestamps are not zero. Preserve the probe diff, exact compiler inputs, workload/case receipts, sample counts and p50/p95 results. Include one/several-pane, replacement, erasure and scroll cases at flat/warped settings, plus bounded-history and expiry behavior. Select the promotion frame budget before examining results.

Run both complete core suites and strict lints on the integrated tree, replay the GPUI patch stack from its pin, and run native representative skin/style checks. Remote CI and the branch's merge policy must pass before integration. Rebuild the actual integrated snapshot before installing; retain rollback to the previously running binary.
