# Status: drawing and hidden output efficiency

Difficulty: 6/10. Cache invalidation and hidden-event delivery can be wrong without an immediate crash.

Parker authorized autonomous fixes, tests and rebuilding while asleep on 2026-10-19. Earlier authorization includes merging and installing. No additional approval gate; final delivery is an annotatable brief with rationale, uncertainty and gain/cost tradeoffs.

- [x] Read current main and earlier experiments; isolate work from other agents.
- [x] Profile current drawing: four visible panes, one busy at 20 Hz; 406 main-thread user samples, none lost. Layout remains prominent. Live-session 19-sample profile is too sparse for ranking.
- [x] Retain only text-grid layout/paint, compare fully transformed rows and geometry. Keep per-frame tube registration and pane chrome outside caching.
- [x] Suppress hidden screen-only wakeups before they reach the UI; preserve parser and non-screen events; recover dirty state from generation on existing housekeeping clock.
- [x] Behavioral tests and deliberate fault controls; native headless GPU smoke with the old host.
- [ ] Finish matched production CPU experiments, including reversed launch order and identical-build controls.
- [ ] Full suites, build, single PR, guarded installation and report.

Least confident: exact row comparison might cost more than reuse saves for continuously changing grids; hidden output callbacks might be a small fraction of CPU; inherited text style and geometry must invalidate GPUI's retained drawing. Whole-pane caching is rejected because warp registration is a per-frame side effect. No longer sleep interval is proposed.

Acceptance: demonstrate skipped quiet-grid render work and fresh changed output; preserve selection/caret/scroll/theme/font/geometry and effects; hidden parse and color replies remain live, completion latency unchanged. Report CPU savings only from matched normal-release binaries. Keep the baseline and all measurements, including regressions. Close or defer an optimization if its benefit is not supported.

The first grid-cache candidate failed output and cursor invalidation tests: child notifications raised during parent rendering arrive after GPUI collects dirty views. Replaced the immutable grid view when final painted content changes; identical snapshots retain the view. The original failures are preserved. All248 pane tests now pass, including40 individually observed hidden writes with zero screen callbacks and unchanged protocol/title/prompt/completion behavior. Full suites and performance are pending.

Both unmodified full release suites passed: 2452 default-core tests and 2405 alternate-core tests, nine ignored in each. Both Clippy configurations and formatting passed. Fault controls removed both optimizations and produced exactly the intended two failing pane tests; source bytes were restored. Initial native capture timed out with physical DPMS off in both baseline and candidate. A temporary headless output allowed real capture without waking the screens; candidate hidden-output activation, document scrolling and GUI-only restart then passed with the old host and unchanged shell identities.
