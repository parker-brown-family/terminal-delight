# Status: HTML Opening Speed

**Difficulty: 6/10.** Opening HTML touches the renderer, cache and native view; stale or misaligned notes can survive a visually fast first frame. One combined plan and one approval.

**Turned out to be: 6/10.** The approved combined plan was sufficient. Native profiling and cancellation/freshness validation needed more work than capture changes themselves.

- Combined plan: approved by Parker on 2026-11-05: “I trust your judgement - go full send and get thei built completely end to end!”
- Implementation: complete. Formatting, Clippy, the full suite (3376 passed; 18 existing skips), native checks and report checks passed. Landing uses one PR for 0.5.3 against dev/0.5.1. The current result is [Prepared Pages Open in 0.2 Seconds](built.html).

## Work

- [x] Trace the current Alt+O and artifact renderer. The relevant docview source is identical to the installed build.
- [x] Establish an isolated checkout from the current development branch.
- [x] Measure current Rust renderer on real glass briefs, separating load, extraction, capture, decode and cache.
- [x] Present the measured findings and one combined implementation plan.
- [x] Answer Parker's resource question before selecting a cache strategy: measure nine retained pages in default/Vulkan, released contexts, profile storage and the live disk cache; add [resource review](resources.html).
- [x] After approval, add native first-visible and viewport-ready timing and fix the largest demonstrated cost.
- [x] Implement viewport-sized capture, visible-first cache reads, dependency validation, browser-context release, hidden-image release, recent-pane preparation, and passage continuity.
- [x] Compare identical inputs and verify notes, links, keyboard navigation, resize and cancellation. Native note creation, automatic saving, resize and reopening passed on the isolated display.
- [x] Complete native 50-pane measurements and the final report; pass format, Clippy and the full suite.
- [ ] Land the single PR for 0.5.3 through dev/0.5.1; observe automatic wave installation.

## Context

Parker reports occasional 2–3 second HTML opening with Alt+O and artifacts. He asks for a 300% performance boost, prefers 500%, and would welcome 10×. Use elapsed-time targets: 4× means 500–750 ms from the reported baseline; 6× means 333–500 ms; 10× means 200–300 ms. The baseline remains a user observation until measured end to end.

The engine already shares Chromium, uses fresh contexts, captures visible tiles first, and caches complete renders. Changing these without measurement would repeat existing work. Browser startup, native display latency and whole-page completion are separate measurements.

The investigation harness is under the APES project scripts directory, `html-open-profile`, and imports the actual renderer sources from this checkout. Native QA now runs a separate Xvfb display. It never takes the live desktop input seat. Implementation is in the isolated worktree `perf/html-open-latency`, physically under `~/Work/td-html-build/terminal-delight`, with its own pinned GPUI checkout.

## Resource Revision

Parker asked where caching lives and what it costs with 30–50 panes, suggesting nine recently visited panes. The diagnostic found 58 cached render versions in 229 MiB, plus 21 MiB extracted media. Nine default-mode live pages consumed 1975 MiB browser PSS; nine Vulkan pages consumed 1245 MiB PSS plus 859 MiB driver memory. Closing contexts reduced browser PSS to 293/475 MiB. Native decoded payload for the sampled nine-view mix is another 281 MiB by exact pixel arithmetic; actual native driver memory remains unmeasured.

The approved implementation keeps byte-bounded disk storage, uses recent panes to rank preparation, releases browser contexts after capture and drops hidden native image buffers. It adds no pinned PNG cache. The resource question was answered before Parker explicitly approved implementation.

Parker also approved exact-visible-area capture, visible-first cache reads, an optional attention-trigger experiment, anchor-based passage continuity and a prepared-open tail target under 50 panes. Focused-pane preparation is implemented; extra hover preparation remains an experiment subject to its measured cost.

## Implementation Findings

- Renderer-only ordinary-brief viewport capture fell from roughly 2.1–2.35 seconds to 0.97–1.29 seconds. This alone does not meet the 4× target.
- Native exact-geometry preparation produced cache hits. Small-square diagnostic paints were 57–77 ms. Full-size unoptimized diagnostic opens were about 300 ms, with a slower tail under 50 panes; these are not the final optimized results.
- A benchmark labelled optimized initially copied a binary overwritten by the full-test build. Its SHA matches the unoptimized artifact, so those samples remain diagnostic only. Final measurements use the explicit optimized artifact path and a preserved copy.
- The process-memory sampler initially walked only the main thread's children, missing Chromium launched from another thread. It now walks every thread. Earlier zero-browser results are invalid, not evidence of zero memory.
- Tests caught a missing source-guard registration and an empty outgoing-render placeholder hiding note targets. Both are fixed. The new cancellation test must let GPUI dispatch the cancelled future before asserting that its guard dropped; its synchronous image-release assertions remain immediate.
- Vulkan remains unselected: the measured repeated cold-browser penalty was 6–8 seconds.

## Final Native Findings

The optimized 50-pane run produced 100/100 cache hits, 191.195 ms median and 263.704 ms p95 to viewport paint. External Alt+O dispatch to log observation was 209.851/311.278 ms. The ordinary cold formal-brief run remained 1727.964 ms median externally; the 4× cold target was not reached and is tracked in [Reduce uncached HTML opening](https://github.com/parker-brown-family/terminal-delight/issues/1372). The plan explicitly permitted continuing preparation after reporting that failed target.

All hidden-image traces reported zero PNG bytes and GPU tiles. Native/browser PSS with 50 panes and the reader visible was 333.1/156.2 MiB. These are whole-process snapshots, not a per-document budget or peak guarantee. Separate native GPU measurements were 60 MiB empty, 96 visible, 64 closed. Production browser idle remains 300 seconds. Additional hover preparation was not enabled; the bounded focused-pane path met its targets without evidence that extra speculative work earns its cost.

One unrelated full-suite process-cleanup test reached its process-scan deadline; the unchanged compiled test passed alone. The load sensitivity is recorded in [Make stubborn-job cleanup validation reliable](https://github.com/parker-brown-family/terminal-delight/issues/1373). Native notes checks initially exposed harness assumptions about autosaving and transient non-JSON loading replies. Correcting the protocol handling produced a passing add/resize/save/reopen check.
