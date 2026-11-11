# Native HTML Opening Measurements

Measured on 5 November 2026 on Parker's machine, in an isolated Xvfb display. No input went to the live desktop. The optimized application uses `CARGO_PROFILE_DEV_OPT_LEVEL=3`, dependency opt-level 2 and the existing patched GPUI revision. The baseline is the installed `f740cb68` build. Final source integrates development through `b081f030`; the timings precede that unrelated keyboard and skills integration.

## Timing

The [summary](summary.json) contains medians, nearest-rank p95 and maxima. The prepared 50-pane sample comprises 100 Alt+O opens at 1234 × 795 CSS pixels, scale 1, with 49 copies of the same HTML brief open in hidden tabs. Each pane runs a shell, not an AI agent. All 100 cache lookups hit. Native viewport paint measured 191.195 ms median, 263.704 ms p95. External key dispatch through observing the log measured 209.851 / 311.278 ms. Native paint is command submission, not measured physical presentation. These are distinct clocks.

The formal brief had 20 prepared opens at 214.886 / 344.645 ms native median/p95. With the disk render cache removed before each of 20 opens, external median/p95 was 1727.964 / 2356.836 ms; baseline was 2000.067 / 3442.251 ms. The browser remains warm between opens. Only the initial sample includes browser startup. Concurrent host work limits paired inference. The cold 4× target was not met. Existing baseline cache repeats were already fast (255 ms median externally); preparation increases eligibility for that path.

Cached files are hot in the filesystem cache. Reads after OS reclamation and physical input-to-display latency are unmeasured. Renderer-only results at scale 1.6 are in [viewport-first.jsonl](../viewport-first.jsonl); they are not native timings. The [harness](native.py), samples, census and memory snapshots sit beside this note. Diagnostic binaries copied from a path overwritten by `cargo test` were excluded; final optimized runs used the explicit optimized artifact, preserved as `/tmp/td-html-native-opt3`.

## Memory

Process PSS is proportional resident memory from `/proc/PID/smaps_rollup`, summed across descendants found through every thread's children. It includes terminal and shell processes. Native/browser MiB in the 50-pane run: empty 220.2/0; nine panes 350.6/252.5; thirty 287.9/207.1; fifty 333.4/162.5; visible reader 333.1/156.2; closed 278.6/155.8. These are snapshots, not peak guarantees. Every hidden-view trace reported zero retained PNG bytes, zero GPU tiles and no live page.

A separate 50-empty-tab run varied enough that subtracting it would give a misleading per-document estimate. Allocators, startup and process reclamation affect whole-process totals. The trace counters establish release of the document-owned buffers; they do not establish zero total RAM per hidden document. Notes, layout, scroll and identity remain.

The [GPU run](gpu/memory.jsonl) reads NVIDIA process memory via `nvidia-smi -q -x`, filtered to owned processes: 60 MiB empty, 96 MiB with the document, 64 MiB closed. It used the existing idle-time override at three seconds to observe shutdown within the test; production stays at 300 seconds. Browser PSS fell from 160.6 MiB closed to zero after idle in the separate cold run. The browser can retain allocator memory while warm even after every context closes.

Preparation uses one worker, nine recent candidates, 400 ms dwell, source ≤32 MiB, page ≤64 million device pixels and PNG output ≤64 MiB. It requires at least 1 GiB `MemAvailable`; failed pressure admission is retried. These checks bound work admitted, not every allocation Chromium may make while loading a page. A current browser call finishes before cancellation is observed. The existing 256 MiB disk sweep is best effort and can be exceeded transiently during writes.

## Fidelity and Behavior

[Fidelity data](fidelity.json) compares the same resource brief and geometry. Page height, all ten complete anchor records and all fifteen link records are equal. ImageMagick's mean absolute difference across the first viewport is 0.0000053805 normalized, or 0.001372 on the 0–255 channel scale. This is a numerical comparison, not a claim of bit identity.

The native functional harness copies the document before editing it. It adds a note through the native controller, resizes the window, verifies the note, waits for the automatic save, closes the reader and reopens it. The record is `functional/exercise.json`. Initial harness assertions incorrectly expected control-socket edits to remain unsaved and Save to succeed on an already saved page; the existing control API saves immediately. A third diagnostic assumed every note-state poll was JSON, although resize can temporarily answer that layout is unavailable. The final harness handles that absent state explicitly and passes the complete note/resize/save/reopen sequence. Those diagnostic failures are not application regressions.

Automated coverage includes viewport-band continuity, stable-anchor passage restoration and ambiguous-anchor fallback, local-asset edits and newly created missing assets, incomplete cache repair, real Chromium resource inventory, queue bounds/coalescing, foreground cancellation, hidden-view cancellation and image release. Existing notes, links, dialogs, keyboard and browser-isolation coverage stays enabled. The final format, Clippy and full-suite results are archived under `validation.json` and accompanying logs.

## Reproduction

Run the canonical harness with an explicitly identified optimized binary and a fresh output directory:

```bash
python3 native.py --bin /tmp/td-html-native-opt3 --brief /home/parker/Work/td-html-build/terminal-delight/docs/plans/html-open-latency/resources.html --out /tmp/td-html-repeat-check --mode reader --prepare-seconds 8 --panes 50 --cycles 100
```

Use `--cold --cycles 20` with one pane for cache misses, `--baseline` with the baseline binary, and `--exercise --cycles 1` for the copied-document note/resize/save check. A baseline has no new native paint instrumentation; unavailable stages remain absent. Xvfb, xdotool, ImageMagick and Chromium must be installed. The harness owns its process group, X display and XDG directories.

Remaining cold latency: [follow-up](https://github.com/parker-brown-family/terminal-delight/issues/1372). Existing process-scan test sensitivity: [follow-up](https://github.com/parker-brown-family/terminal-delight/issues/1373).
