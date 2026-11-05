# Plan: HTML Opening Speed

**Difficulty: 6/10.** This is the historical combined plan, approved by Parker on 5 November 2026. Implementation results are in [Prepared Pages Open in 0.2 Seconds](built.html). The following proposal text is retained as the approval record; it does not describe current completion status.

## What It Is For

Alt+O and artifact opening should show a readable page promptly. Parker reports occasional 2–3 second waits. The source-backed headless probe reproduced a 2.35 second viewport on a recent glass brief. The goal is at least 4× faster opening on the agreed ordinary-brief corpus, with 10× as the target for prepared or repeated opens. These are targets, not achieved native UI results.

## Evidence

The renderer source on the isolated checkout is identical to the installed build's docview source. Four actual HTML files were measured three times each at 968 × 1400 CSS pixels and scale 1.6, using current renderer modules in a small release-mode Rust harness. Browser launch was measured separately. Its sequential capture/decode timings omit native dispatch, GPU upload and display presentation; native decoding may overlap capture. These are preliminary component measurements, not a keystroke-to-screen benchmark or a p95 estimate.

| Page | Current viewport | Vulkan viewport | Ratio |
|---|---:|---:|---:|
| Formal verification brief | 2351 ms | 646 ms | 3.64× |
| Desktop contention brief | 2135 ms | 603 ms | 3.54× |
| Agent identity brief | 2298 ms | 831 ms | 2.77× |
| Chalk study, 26.5 MB | 2657 ms | 1941 ms | 1.37× |

Medians of three. The first Vulkan page took 8254 ms and, in a second browser, 6166 ms. Both exclude the separately measured browser start. The median conceals this recurring cold penalty. Merely adding `--enable-gpu` did not materially solve capture latency on this host. Raw samples, the harness and its lockfile are under [evidence](evidence/summary.json).

Existing behavior already includes a shared browser, fresh page contexts, visible tiles captured first, content-keyed disk cache, background decode and bounded GPU residency. Current cache reads took 0.6–5.8 ms; decode, file validation and native upload still cost time. Current exact viewport keys mean a different frame geometry misses that cache.

Parker requested resource measurements before choosing a strategy for 30–50 panes, suggesting the nine most recently visited panes. The [resource study](evidence/resources.md) found 58 render versions occupying 229 MiB within the existing shared 256 MiB disk budget. Nine retained browser pages cost 1975 MiB PSS in the current mode, or 1245 MiB PSS plus 859 MiB NVIDIA memory with Vulkan. Closing those contexts reduced browser PSS to 293/475 MiB respectively. Native image payload for the nine-view mix adds 281 MiB decoded RAM and 48 MiB PNGs; native graphics allocation is unmeasured. Thus recency selects preparation priority; it does not justify retaining nine live browser pages.

## The Shape

Keep the snapshot engine and the brief's existing appearance, notes, links and keyboard behavior. Work in the isolated checkout, with one implementation PR against the then-current development branch and milestone.

1. **Measure the native open.** Add optional `OpenTiming` observations at document request, settled geometry, cache lookup, layout, first tile, decode, first paint and viewport coverage. Unobserved stages remain absent. Trace both Alt+O and artifact rail opens on an isolated display, including cache misses, cache hits, a fresh browser, resize and a tall document. Native first paint and complete viewport are separate results.
2. **Make capture cheaper.** Qualify the measured Vulkan path on this machine, including image and anchor equivalence and the recurring first-capture penalty. Benchmark capturing only the visible pixel area before adjacent tiles. Preserve existing software behavior on unsupported or failing GPU configurations. Do not promote an accelerated path whose cold result is worse than the current path; first establish whether renderer preparation or shader-cache reuse removes that penalty without retaining page storage. A cache entry includes the renderer policy/version when output differs.
3. **Bound retention before preparing artifacts.** Close browser contexts when capture is complete, using the existing dialog-reopen path when needed. Release decoded images and PNG copies for hidden documents while retaining notes, scroll and document identity. Share valid completed disk entries; add no pinned PNG memory cache without a measured benefit. Measure native reactivation and transient memory before selecting a grace period or byte ceiling. Keep the shared browser's existing idle shutdown unless evidence supports changing it.
4. **Prepare the next known artifact.** Rank candidates from up to nine recently visited panes, prioritizing the focused pane's newest declared handover. Recency is a candidate list, not a command to render all nine. Reuse the existing renderer and completed disk cache, with one cancellable preparation job per window, coalesced by path/content/geometry. Close the preparation context at completion. Keep the existing shared 256 MiB disk budget. Opening takes priority over preparation; no directory crawl or eager rendering of every artifact. Exact geometry and source/dependency freshness must match before reuse. Width changes or edits cause a real miss. Prepare the next likely viewport only after the native trace establishes its geometry. A single job is not a memory cap: include resource admission/cancellation for expensive pages and measure the peak.

Interfaces to add or tighten, without implementation bodies:

```rust
struct OpenTiming { /* observed stage durations: Option<Duration> */ }
struct RenderKey { /* content, dependencies, geometry, renderer policy */ }
enum RenderPriority { Opening, Prepare }
fn prepare(request: PageRequest, key: RenderKey, priority: RenderPriority);
```

Primary source scope: `app/src/docview/page.rs`, `snapshot.rs`, `cache.rs`, `engine.rs`, `pref.rs`, document-opening call sites in `docview.rs`/`docopen.rs`/`pane.rs`/`main.rs`, and focused renderer tests. Add no AI calls or replacement browser engine. The native trace determines the smallest necessary call sites.

## Experience Refinements Proposed on 5 November

Parker asked what else could make this impressive after reviewing the resource strategy. These are proposals within the same 6/10 combined plan; they do not constitute approval or measured improvements.

- **Complete visible-area capture and cache reads first.** At the test geometry, 1400 CSS rows × 1.6 = 2240 device rows. Current 2048-row bands capture 4096 rows before filling that viewport on a sufficiently tall page at the top. Capturing the exact viewport offers 45.3125% fewer first-screen pixels and potentially one capture call instead of two. Benchmark actual latency and total work, including integration with subsequent cache bands; do not infer proportional elapsed-time gains. Existing cache-hit code reads every PNG before adopting the layout. Adopt validated geometry and load/decode visible bands first; remaining bands must preserve current completeness/freshness rules.
- **Prepare on explicit attention as an experiment.** After the focused-pane path works, compare a short artifact-hover dwell or keyboard selection with recency alone. One cancellable job, foreground priority, no inference service. Measure useful hits, wasted captures, peak memory and foreground latency before enabling it. Choose dwell duration from those results; it is not set here.
- **Preserve the passage across revisions.** `PageDoc::land` currently keeps a fractional scroll position; `adopt` already carries notes and keeps outgoing visible images until replacements cover the view. Prefer the nearest stable anchor plus within-section offset when both revisions contain that anchor. If absent or ambiguous, use a deliberate fallback without inventing a match. Pixels, note locations and clickable geometry must belong to the displayed version. Test an insertion above the viewport, reordering, deletion of the anchor, resize, active unsaved notes and delayed old captures.
- **Give slow opens and input delay explicit acceptance limits.** Keep the 250 ms median prepared-open target; propose p95 ≤500 ms under the isolated 50-pane workload, with at least 100 prepared opens per workload for a more useful tail sample. Record input-to-feedback, scrolling and cancellation separately from page readiness. A current Chromium call may need to finish before cancellation, so responsiveness must be measured rather than claimed as instantaneous preemption.

Preferred order: the existing trace, visible-area capture and memory limits; passage continuity; then attention-triggered preparation if the measured hit/waste tradeoff supports it. No additional permanent image cache is introduced by these proposals.

## Least Confident Decisions

- **Vulkan readiness:** repeated fresh browsers incurred 6–8 seconds. Warm gains alone cannot justify changing the default. Preparation/cache experiments must remove that regression or leave Vulkan unselected.
- **Preparation's cost:** nine live contexts already consume gigabytes in the resource probe. Prepare one likely artifact and release its context. Peak memory during capture, native visibility/revival cost, and typing/scroll responsiveness remain unmeasured. The existing 256 MiB disk budget is a measured useful starting point; nine recent panes is a proposed priority policy, not an approved count limit on browser pages.
- **Freshness:** today's cache validates the HTML bytes and exact geometry. Prepared renders must also handle changed local assets. A mismatched or unknown dependency state is a miss, never permission to show stale notes or pixels.
- **10× scope:** preparation has a plausible path to 200–300 ms; an arbitrary large page opened for the first time does not yet have that evidence. Report these separately.

## Slices and Acceptance

1. Trace and baseline the native path. Record at least 20 repeats per selected open scenario; distinguish first browser launch, warm miss and valid cache hit, including hot filesystem-cache reads and reads after reclamation. The preliminary 0.6–5.8 ms cache values followed writes and describe only the hot case. Use original unchanged pages and saved exact build/config metadata.
2. Ship the largest proven capture improvement with cold-path and visual checks. The first ordinary-brief target is median viewport-ready time ≤25% of the paired current baseline, without a worse cold p95. If this fails, retain the measured result and continue the planned preparation slice; never report a target as met.
3. Release completed browser contexts and hidden native buffers, then prepare one likely artifact and consume only a valid completed render. Target ≤250 ms median for prepared opens from the reported 2–3 second case. Verify edits, linked asset changes, rapid switching, cancellation, resize, fragments, notes, dialogs and concurrent opens. Run isolated 9/30/50-pane workloads to prove background pages and hidden pixel buffers do not grow with total pane count. Record peak browser/native RAM, driver memory, idle behavior and foreground responsiveness. Existing browser isolation/network boundaries remain tested. The 30/50 resource projections are not acceptance evidence.
4. Run required format, Clippy and full tests, compare real screenshots and native latency, and land one PR only after its evidence supports the claimed gains. Follow current release instructions and automatic wave installation; the investigation does not replace the live window.

Approval covers these slices. It does not establish their results in advance. No product code, preferences or running window changed during this investigation.
