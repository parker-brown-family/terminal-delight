# Terminal viewport efficiency

Difficulty: 6/10. A bounded styling cache can silently keep stale selection, colours or output; correctness and comparable measurements decide whether it ships.

Approved: 2026-10-17. Parker concurred with “Begin with hidden-pane invalidation” in reports/2026-10-17-td-efficiency.html and said “Let's see how far we can push it, yes!” He also approved exploring reuse. This is the combined plan under that approval, updated against current main rather than repeating shipped changes.

Current main a5fab29 already includes hidden-pane throttling, live-row copies, visible-output limiting, stepped badges and bounded rollout reads. A sibling's isolated 30-hidden-pane diagnostic measured 3.4–3.7% idle versus 6.1–6.2% busy of one logical CPU. This is prior evidence, not a new before/after result.

Implementation: skip terminal styling on workbench/document faces; retain one styled viewport per pane and reuse only when the actual cells, selection, cursor, scroll offset, dimensions, agent highlighting, inversion and resolved theme all match. Compare real cell snapshots rather than trusting an event counter to cover every UI mutation. Keep layout, text shaping, terminal emulation, badge cadence and input handling intact. No added dependencies or installs.

Validation: behavioural tests compare cached and uncached styled output after mutations and prove reuse on unchanged redraws; show that covered faces do not style the grid and switching back catches up. An ignored repeatable benchmark measures styling separately from the whole window. Run alternating isolated window measurements with identical baseline/candidate workloads, disclose unknown latency and workload limits, and retain results even if savings are small. Required formatting, Clippy and both core suites precede review.

The approved reuse direction also covers an existing documented problem: an unchanged document is rebuilt on every parent frame (GitHub issue 725 — floating Markdown rebuilds). Reuse GPUI's built-in AnyView cache with a full-size layout style for floating/document-face views. Existing notify paths invalidate content/scroll/theme; GPUI additionally keys on bounds, text style and clipping. A render-count behavioural test must prove quiet reuse, scroll/theme invalidation and geometry invalidation; existing real-pane tests cover clicks, selection, notes and file reloads. This extends validation, not the runtime/dependency model.

Terminal Lab's separate pending plan is outside this change. Original annotated report and other agents' worktrees stay untouched.

Status: implementation and local validation complete. Five paired normal-release samples per workload: window CPU medians 43.87→41.00% terminal, 35.62→30.37% workbench, 59.00→40.12% with the document floating, all percent of one logical CPU. Every paired interval improved. Both core suites passed (2401 and 2354 tests, nine ignored each), both strict Clippy checks passed, formatting passed. The four-figure annotatable result page passed 1100/720/390px browser checks including note/export/concur and the evidence dialog. The narrative gate reports argument, not catalogue; heading coherence 0.79. The original annotated brief is untouched. No installation or live-window replacement. Post-hoc difficulty: 6/10.
