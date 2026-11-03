# Promotion slices

- [x] Establish selected 0.5.0 routing and integrated baseline; record current daily preview and coordinate deployment.
- [x] Add automated skin/design/palette and retained-grid regressions with a failing control.
- [ ] Measure native rendering cost and verify representative skins, exact zero and bounded expiry.
- [ ] Pass both full core suites/lints and fresh GPUI replay; merge the same feature PR into dev/0.5.0.
- [ ] Build/install the integrated daily snapshot, preserve live sessions and document the exact delivered scope and hindsight.

Each slice follows Parker's explicit shipping instruction; no additional interactive approval is required. Stop on an actual unresolved failing test, incompatible host wire or unsaved document state, and make the concrete failure reviewable.

The real pane matrix passes all 100 embedded skin/design/glass/crawl combinations, asserting actual GPU tube values, focus text, complete selected-colour erasure, 500ms visibility, 650ms expiry and explicit zero controls. A separate passing test enumerates every bundled palette against every design, flat/tubed states, inherited settings and persisted zero pins. Registries supply the inventories, so new entries join automatically.

Two fixture assumptions were corrected without changing production behavior: the headless platform drops native frame callbacks, so the expiry test delivers the same child notification explicitly; and font/design changes stage a real 140ms PTY resize. The harness now waits for pending size work to finish before measuring an erasure. The observed 100×28→114×32 resize correctly invalidated history; a content-only fixture must not overlap it. An initial test setup used an Option for the existing boolean CRT field; that compile error was fixed. All final targeted checks pass.
