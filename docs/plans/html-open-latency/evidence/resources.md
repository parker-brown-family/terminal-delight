# HTML Resource Measurements

Measured on 5 November 2026 with the same renderer revision, Chromium binary and geometry as the [latency study](README.md). No production source or preferences changed. The diagnostic processes used their own headless browser and temporary profile; no desktop input was injected.

## What Is Stored Where

- Original artifacts remain at their authored paths. `Pane::declare_deliverable` only assigns the link/label record; see [pane.rs](../../../../app/src/pane.rs). Declaring 50 artifacts does not open 50 browser pages.
- Complete render cache: `$XDG_CACHE_HOME/terminal-delight/pages`, or `~/.cache/terminal-delight/pages`. It contains layout JSON, compressed PNG bands, cached dialogs and extracted media. The 256 MiB cap covers the shared directory, not each pane. `cache::sweep` removes least recently used entries after writes; in-progress writes can exceed the cap. See [cache.rs](../../../../app/src/docview/cache.rs).
- Browser profile: `$XDG_RUNTIME_DIR/terminal-delight`, normally `/run/user/1000/terminal-delight`, or the temporary-directory fallback. Profiles are removed after browser shutdown; startup also removes stale profiles whose owners died. The runtime directory is memory-backed on this host. The harness selected a private `/tmp/td-html-resource-profile-<pid>` directory instead. See `runtime_dir`, `fresh_profile`, `finish`, and `remove_stale_profiles` in [snapshot.rs](../../../../app/src/docview/snapshot.rs).
- One `SnapshotEngine` is cached per TD application process/window. It starts Chromium lazily, opens a separate browser context per page, and shuts the whole browser down after 300 seconds without an engine call. Activity in another page can postpone that timeout. See `Engines` in [docview.rs](../../../../app/src/docview.rs) and `engine_thread` in snapshot.rs.
- Each open `PageDoc` retains PNG bytes for all captured bands, decoded `RenderImage` objects for visible/neighbor bands, and any live browser page. `release` drops them when the document view is released. Pane fields retain the view while its tab or document face is hidden; no visibility-based release exists in the inspected path. `reveal` concerns Alt-key note outlines, not document visibility. See `TileSet`, `plan_residency`, `release`, `open_dialog` in [page.rs](../../../../app/src/docview/page.rs), and `show_document`/`doc_on_face` in [pane/doc.rs](../../../../app/src/pane/doc.rs).
- A cache-hit open reads PNGs and layout without opening a live browser page. Dialogs can recreate the browser page on demand; the existing `open_dialog` path already handles a closed or idled-out browser.

## Experiment

[resources.rs](resources.rs) imports the actual renderer modules, warms one browser, and retains nine freshly rendered pages. It cycles through the formal verification, desktop contention and agent identity briefs three times. Reusing these three sources tests nine separate browser contexts, not nine distinct artifacts; browser sharing may differ with a diverse corpus. Every band is captured. PNG bytes remain in the harness while all page contexts are closed and the browser is subsequently shut down.

[resources.py](resources.py) takes process-tree measurements at each checkpoint. It sums `/proc/<pid>/smaps_rollup` PSS, which apportions shared physical RAM instead of counting it once per process. The tree excludes the Rust harness, hence PNG buffers and TD native image buffers are excluded from browser RAM. NVIDIA memory comes from the matching processes in `nvidia-smi -q -x`. The successful driver query reported zero NVIDIA allocation for the default renderer; this says nothing about TD's native display textures. RAM and driver memory are separate observations and should not be summed as a unique physical-memory total.

Each checkpoint waits 0.5 seconds, then samples over roughly one second. These are retained-state snapshots, not peak-memory measurements. The short CPU observations ranged from 0–6.9% of one core for default rendering and 0.8–5.4% for Vulkan between render jobs. They are not a steady-state or battery benchmark. Capture itself does substantial work; wall times in the resource run omit native decoding/display and are not replacements for the latency study.

One run per mode, alongside other machine work. A diagnostic stop at 3 GiB browser PSS was never reached. All nine page contexts were measured in both modes. After shutdown, no owned child processes remained, driver allocation was zero, and the private profile directory was removed.

| State | Default browser PSS | Vulkan browser PSS | Vulkan NVIDIA memory |
|---|---:|---:|---:|
| Warm browser, no page | 160.6 MiB | 337.6 MiB | 14 MiB |
| One fully captured page | 498.2 MiB | 526.0 MiB | 201 MiB |
| Three retained pages | 920.5 MiB | 747.5 MiB | 547 MiB |
| Nine retained pages | 1975.2 MiB | 1245.5 MiB | 859 MiB |
| All pages closed, browser kept | 292.8 MiB | 475.4 MiB | 25 MiB |
| Browser shut down | 0 MiB | 0 MiB | 0 MiB |

Raw runs: [default](resources-default.jsonl), [Vulkan](resources-vulkan.jsonl). [Resource summary](resource-summary.json) retains checkpoint values, per-file dimensions and projections. [Cache census](cache-census.json) records the existing on-disk entries without modifying them: 58 render versions, 229.0 MiB; six media entries, 21.0 MiB; total 250.0 MiB. Entry sizes are logical file lengths, not filesystem allocation. Median render 3.28 MiB; nearest-rank-style sampled 95th order position 7.71 MiB; maximum 22.20 MiB. No page identities are inferred from cache entry names.

## Native Image Accounting and Scale

At 968 CSS pixels and scale 1.6, image width is 1549 device pixels. One full 2048-row BGRA band holds `1549 × 2048 × 4 = 12,689,408` bytes, or 12.10 MiB. At the top, the actual residency planner retains two visible bands plus the next one, where present. The short contention page has only two bands. The three-page mix therefore retains 93.59 MiB of decoded pixels, or 280.78 MiB for nine pages. Compressed PNGs for nine total 48.09 MiB. These byte counts exclude layout metadata, allocator overhead, temporary decode copies and dialogs.

The usual five-band ceiling is 60.51 MiB of decoded pixel payload for a long document at this width. A viewport requiring more than five visible bands is allowed to exceed the ceiling, and resize briefly retains old/new renders together. GPUI `RenderImage` holds the BGRA frames in CPU memory; the WGPU atlas uploads images separately when painted. Native driver allocation was not measured. Its texture packing, upload timing and overhead make it inappropriate to claim that every decoded byte is already a GPU byte.

The 30/50-page browser projections use `warm + N × (nine − warm)/9` from the default run: approximately 6.06/10.0 GiB PSS. Decoded payload scales to 936/1560 MiB. These estimates assume similarly sized documents retained before global browser idle shutdown; they are not measured large-pane workloads. Real smaller tiled panes may be cheaper, and large image-heavy pages may be more expensive. A page reopened from a valid completed cache has no live browser context until something needs one.

## Consequence for the Plan

Keep the existing byte-bounded disk cache. Rank preparation candidates using up to nine recently visited panes, with the focused pane first; do not pre-render all artifacts on declaration or batch all nine. Run one cancellable background job, close its browser page after capture, and retain the completed snapshot. Release decoded images and PNG copies when documents become hidden, preserving notes, scroll and document identity. Measure native reactivation cost before choosing any grace period or extra in-memory cache. Cache reads already took 0.6–5.8 ms in the latency probe, leaving little demonstrated benefit for another pinned PNG cache.

The 0.6–5.8 ms cache reads immediately followed writes and thus benefited from Linux's filesystem cache. Reads after memory pressure or eviction from that cache are unmeasured. This is a reason to begin without a duplicate pinned cache and measure, not proof that all disk reads will take those times.

Implementation must measure transient peak memory, foreground latency and typing/scroll responsiveness, including hot file-cache reads and reads after reclamation. A single background job limits concurrency but is not a byte guarantee. Image-heavy pages need an admission/cancellation rule and explicit peak measurements. These remain implementation gates in the revised plan, not claims of a shipped resource limit.

To reproduce from the canonical harness directory, build both binaries with `cargo build --release --offline`. Set `RESOURCE_PROBE` if the build runner puts the executable outside `target/release`; lean-ctx placed it in `~/.lean-ctx/build-cache/cargo-target/release`. Run:

```bash
python3 resources.py /tmp/td-resources-default.jsonl default /home/parker/Work/reports/2026-11-04-td-formal-verification.html /home/parker/Work/reports/system-contention-2026-11-04.html /home/parker/Work/terminal-delight/reports/2026-11-02-agent-identity-built.html
```

Repeat with `vulkan` and a different output path. The source paths are recorded in [manifest.json](manifest.json); the resource probe reads the same renderer checkout as the latency probe.

## Source Check for the Experience Proposals

On 5 November, inspected `PageDoc::start`, `adopt`, `land`, `next_fetch`, `plan`, `TileSet`, and `SnapshotEngine::tile` in the same checkout. Visible-first fetching, retention of outgoing visible pictures during refresh, and carry-forward of unsaved notes already exist. The proposed changes extend those behaviors rather than claiming them as new.

The first-screen pixel calculation applies to a tall page opened at the top: two 2048-row bands cover the 2240-row viewport, leaving 1856 captured rows beyond it. `1 - 2240/4096 = 45.3125%`. The engine already supplies an explicit capture clip; an exact-viewport experiment requires proper band/cache handling, not a claim that simply changing one argument preserves all invariants. Capture, layout, compression, decoding and driver overhead need not scale linearly with pixels. No new timing experiment was run for this proposal.

`PageDoc::start` reads all cached tile PNGs before adopting a hit. Reading visible bands first is proposed; the whole cached entry must still validate, and missing/evicted files during consumption must recover normally. `PageDoc::land` multiplies scroll by the ratio of new to old document height unless a fragment was requested. Stable section identity and offset could preserve the passage more precisely across insertions above it. This is a product refinement, not evidence of a crash or data loss.

Attention-triggered preparation and p95 ≤500 ms for prepared opens under 50 panes are hypotheses and proposed acceptance criteria. They add no measured performance claim to this report.
