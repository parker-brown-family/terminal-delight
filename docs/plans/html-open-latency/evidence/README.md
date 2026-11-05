# HTML Opening Measurements

Measured on Parker's machine on 4 November 2026 Pacific, with Chromium 152.0.7977.82 and an optimized Rust harness compiling the actual renderer modules. Source checkout: `/home/parker/Work/td-html-open-latency`, revision `8913e9501239900b767b5db77770af06711e5674`. `git diff 556a0d7f..HEAD -- app/src/docview` was empty; the relevant source matches the installed build.

Each file was opened in a fresh browser context three times. Each process owns its browser/profile and shuts it down at completion. Browser startup is recorded separately. Every 2048-device-pixel band was captured, decoded to RGBA and swizzled to BGRA sequentially. `first_ms` ends after the first decoded band; `viewport_ms` ends after sufficient decoded bands cover 1400 CSS pixels at scale 1.6. This excludes native GPU upload and display, and serial decode can overstate the actual pipeline where decode overlaps later capture. All-page completion is separate. No live desktop input was used.

`baseline.jsonl` uses current default preferences. `vulkan.jsonl` uses the existing `gpu = "vulkan"` preference in the harness only. `vulkan-restart.jsonl` repeats the first page after a fresh browser/profile. `enabled-gpu.jsonl` adds only Chromium's documented `--enable-gpu` flag through a disposable wrapper. The extra enabled-GPU corpus, retained as `enabled-gpu-corpus.jsonl`, also failed to approach the Vulkan warm results.

The first Vulkan viewport took 8254 ms; a fresh-process repeat took 6166 ms. Subsequent samples took 579–663 ms for the same page. This cold regression is why the existing Vulkan preference was not enabled for the user.

The copied harness imports source files by absolute path from the isolated checkout. Its canonical runnable directory is `/home/parker/BROWN-FAMILY-SPORTS/Software/apes/projects/terminal-delight/scripts/html-open-profile`. Run there:

```bash
python3 html-open-profile.py --gpu default --out /tmp/td-html-baseline.jsonl /home/parker/Work/reports/2026-11-04-td-formal-verification.html /home/parker/Work/reports/system-contention-2026-11-04.html /home/parker/Work/terminal-delight/reports/2026-11-02-agent-identity-built.html /home/parker/Work/terminal-delight/reports/2026-10-20-chalk-marker-and-depth.html
```

Repeat with `--gpu vulkan` and a different output path. `summary.json` is the per-file median of three; it is not a p95 measurement. Broad timing variation and three samples limit precision. The probe ran alongside other machine work. Hashes and sizes of the sampled source files are retained in `manifest.json`.

Chromium's [headless GPU documentation](https://chromium.googlesource.com/chromium/src/+/refs/heads/main/docs/gpu/using-gpu-hardware-in-headless-chrome.md) explains that headless defaults force software rendering, that `--enable-gpu` restores driver detection, and that Vulkan can work on some Linux configurations. Our measurements determine the result on this machine; that guidance does not establish performance or fidelity here.

No production source or application preferences were changed. Visual equivalence and native keystroke-to-screen measurements remain implementation gates.

Cache reads immediately followed writing the same completed render. The 0.6–5.8 ms values therefore describe files hot in Linux's filesystem cache. Reads after reclamation or memory pressure were not measured; native acceptance must distinguish those cases.
