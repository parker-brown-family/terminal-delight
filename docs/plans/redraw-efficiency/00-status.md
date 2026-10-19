# Status: drawing and hidden output efficiency

Difficulty: 6/10. Cache invalidation and hidden-event delivery can be wrong without an immediate crash.

Parker authorized autonomous fixes, tests and rebuilding while asleep on 2026-10-19. Earlier authorization includes merging and installing. No additional approval gate; final delivery is an annotatable brief with rationale, uncertainty and gain/cost tradeoffs.

- [x] Read current main and earlier experiments; isolate work from other agents.
- [x] Profile current drawing: four visible panes, one busy at 20 Hz; 406 main-thread user samples, none lost. Layout remains prominent. Live-session 19-sample profile is too sparse for ranking.
- [x] Retain only text-grid layout/paint, compare fully transformed rows and geometry. Keep per-frame tube registration and pane chrome outside caching.
- [x] Suppress hidden screen-only wakeups before they reach the UI; preserve parser and non-screen events; recover dirty state from generation on existing housekeeping clock.
- [x] Behavioral tests and deliberate fault controls; native headless GPU smoke with the old host.
- [x] Matched production CPU experiments, reversed launch order, identical-build controls, saturated output and a plain-program hidden control: 55 pairs retained.
- [x] Both full suites, both strict Clippy checks, release build, integrated native smoke and a single pull request with an annotatable report.

Release state: PR #1000 records the merge/check status. The machine-specific GUI cutover receipt and final delivery page live under `/home/parker/Work/reports/2026-10-19-redraw-efficiency-evidence/` and `/home/parker/Work/reports/2026-10-19-td-redraw-efficiency.html`; source-tree validation does not assert that a particular machine has installed it.

Least confident: exact row comparison might cost more than reuse saves for continuously changing grids; hidden output callbacks might be a small fraction of CPU; inherited text style and geometry must invalidate GPUI's retained drawing. Whole-pane caching is rejected because warp registration is a per-frame side effect. No longer sleep interval is proposed.

Acceptance: demonstrate skipped quiet-grid render work and fresh changed output; preserve selection/caret/scroll/theme/font/geometry and effects; hidden parse and color replies remain live, completion latency unchanged. Report CPU savings only from matched normal-release binaries. Keep the baseline and all measurements, including regressions. Close or defer an optimization if its benefit is not supported.

The first grid-cache candidate failed output and cursor invalidation tests: child notifications raised during parent rendering arrive after GPUI collects dirty views. Replaced the immutable grid view when final painted content changes; identical snapshots retain the view. The original failures are preserved. All 248 pane tests now pass, including40 individually observed hidden writes with zero screen callbacks and unchanged protocol/title/prompt/completion behavior. The complete validation and measurement results follow below.

Both unmodified full release suites passed: 2452 default-core tests and 2405 alternate-core tests, nine ignored in each. Both Clippy configurations and formatting passed. Fault controls removed both optimizations and produced exactly the intended two failing pane tests; source bytes were restored. Initial native capture timed out with physical DPMS off in both baseline and candidate. A temporary headless output allowed real capture without waking the screens; candidate hidden-output activation, document scrolling and GUI-only restart then passed with the old host and unchanged shell identities.

Final integrated validation: 2465 default-core tests and 2418 alternate-core tests passed, nine ignored in each. Both Clippy configurations, formatting and the native old-host smoke passed. First independent CI passed both full configurations and release smoke. Final report/evidence updates leave the application tree unchanged.

Mixed visible panes saved a median 4.75 CPU points in the initial five pairs and 5.12 points with launch order reversed (about28–29% relative). All-busy 20 Hz and 100 Hz fixtures improved in all five pairs each. The plain hidden writer control saved a median 1.40 points, about29% relative, while idle stayed at a median 1.70%. The synthetic-agent workload improved in only 6/10 busy pairs with about 2% median paired reduction; no dependable real-agent CPU gain is claimed.

The synthetic Codex filename reproduces existing the agent-classification issue (#901): ordinary Python processes become CODEX and invoke conversation lookup. The old compatibility host also predates bounded rollout-header reads. Both original hidden series are retained; one wrapper ended with a final syntax error after all ten observations because its file was edited while running, and its EXIT cleanup was verified. The control uses worker.py and verifies python3 classification. Remaining agent-path work belongs to the agent-classification issue (#901) and hidden-pane CPU issue (#964).

Post-hoc difficulty: 6/10. The immutable child boundary needed adversarial same-frame tests; timing required controls for rendering, launch order and false agent classification. Resident-memory attribution, typing-latency tails and whole-machine/battery effects remain unmeasured.
