# Terminal Delight styling reuse — reproduction and limits

The final comparison is `samples.jsonl`: five paired eight-second intervals for each of terminal, workbench and floating-document workloads. Pair order in each record is baseline window, baseline session host, candidate window, candidate session host. CPU is percent of one logical CPU, summed across the process's threads. PID plus kernel start ticks are preserved, as are raw process counters. `summary.json` contains the three workload summaries.

Both executables were built with `cargo build --release --locked`, using the existing toolchain and dependencies. The baseline is main a5fab29 — Merge the direct-human-turn navigation fixes. `provenance.json` records each source and executable hash. No package or plugin was installed. No installed launcher or running personal window was replaced.

## Run the same workload

From the candidate worktree, with the baseline and candidate binaries built normally:

```bash
bash reports/2026-10-17-td-styling-evidence/run.sh /tmp/td-efficiency-baseline-a5fab29 /tmp/td-efficiency-candidate-production
```

This is a machine-specific diagnostic, using the existing hidden-window helper and the captured Hyprland/Wayland session. It makes two isolated sessions, one per binary, on separate hidden workspaces with rendering enabled; focus and the user's layout are left alone. It checks four panes, one marked busy, and actual readiness text from all four terminals before sampling. The streams run independently in each session. The target is twenty status-line updates per second in one pane; three retain the same text and sleep. All four panes explicitly pin CRT off. Actual output frequency and grid-cell dimensions are unmeasured; equal saved layouts and window geometry are retained. The document case opens the identical 120-section Markdown file over the busy pane. Face order alternates by round, with three seconds of settling before each sample. Hosts are measured separately. Every successful or failed run cleans only its own windows, session hosts and session state.

No profiling, compilation or test run overlapped the final CPU intervals. Ordinary user workloads continued. A pair shares one machine and one GPU concurrently; this reduces time drift but does not establish results on other machines or with real agents. Input-to-presentation latency, GPU timings and badge latency are unknown. This fixture does not claim a speedup for thirty hidden agents.

`null-control/` is a one-round baseline-versus-baseline check: terminal and workbench differences were below 0.4 CPU points. `preflight/` and `test-build-diagnostic/` are excluded from claims. The latter used the test build's GPUI dependency graph rather than a normal production build. Setup failures produced no accepted measurements.

## Correctness and build checks

`validation.json` preserves the suite counts and hashes of the complete local logs below. The repository ignores `.log` files; those raw files are retained on this machine, with fresh CI output available from the pull request.

- `tests-default.log`: 2295 unit and 106 integration tests passed; nine ignored overall.
- `tests-fallback.log`: 2248 unit and 106 integration tests passed; nine ignored overall.
- `clippy-default.log` and `clippy-fallback.log`: both strict production Clippy checks passed.
- Formatting and `git diff --check` passed.
- New real-pane tests exercise content, selection, cursor, alternate screen, scroll, theme, font, grid dimensions and caller-copy independence; covered faces skip styling and switching back catches up.
- The document render-count test proves quiet reuse, a forced GPUI refresh as a cache-bypass control, and invalidation for scroll, palette and bounds. Existing full-suite tests exercise document pointer actions, notes, selection and reloads.

The ignored `styled_viewport_benchmark` reports only the styling path with syntax colouring enabled. It is not a whole-window CPU benchmark and is not used to calculate the reported window savings.

## Separate desktop investigation

`desktop-recheck.json` is a fifteen-second desktop-service counter sample after Burnbar removal. It includes the compositor, shell and descendants. Its reaped-child counter can include CPU accrued before the interval, and must not be added to the service counter or attributed wholly to application scans. The unresolved scan trigger is tracked in https://github.com/parker-brown-family/omarchy-contrib/issues/5.
