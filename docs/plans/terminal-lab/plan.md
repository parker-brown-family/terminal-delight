# Plan: Terminal Lab

**Difficulty: 5/10** — one combined implementation gate. Scope: reusable measurements and a bounded correctness pilot.

## What it is for

Parker approved performance first and a bounded weekly GitHub lab in the Terminal Lab brief. Produce reproducible evidence about the real terminal, then make deterministic tests find exact failures. A performance demonstration must state the workload and what was measured. Fifty live agents and fifty terminal sessions are different claims.

## Current basis

The installed binary is td-0c10d9b-human-prompt-arrows, an install tree including current main's performance changes. The shared checkout is usage-pace-burndown, an older branch. Build implementation in an isolated worktree from current origin/main, preserving this checkout and the running session host.

Already shipped:

- PR #967 — A pane in a tab that is not shown asks for a frame at most every five seconds.
- PR #971 — A pane's live screen is copied once per change; hidden screen checks run once a second.
- PR #974 — Visible output asks for a frame at most every 33 ms, with keystroke echoes bypassing the cap.
- PR #980 — Tab badges step on a 175 ms clock; the preceding output cap did not reduce the live profile's drawing share.
- PR #982 — Matching a Codex rollout reads at most its first 4 KiB.

These PRs are completed; do not reimplement them. Their profile shares and differently configured historical rigs are not one comparable before/after series.

This turn revalidates 30 hidden status-output panes plus two idle panes in one isolated session, with the final idle tab selected. Alternate idle/busy twice; six-second settling and ten-second interval CPU counters. Raw records: reports/2026-10-16-terminal-lab-evidence/. Results and limits belong on the accompanying decision page.

## The shape

### Performance lane

Reuse scripts/lib/hidden-window.sh and scripts/td-echo-bench.sh. Preserve the diagnostic run's source, binary hash, layout and interval counters. Convert it into a bounded reusable runner in scripts/lab/performance.py, with a small shell entry using the existing hidden-window helper where needed. Add no package dependencies.

Each record carries declared workload, pane counts, visible count, CRT state, grid dimensions, output rate, build revision/hash, machine/driver/session information, interval length, CPU for window and host separately, proportional memory and pressure counters. Unavailable readings stay absent/null with a reason. A failed setup or incomplete sample cannot pass.

First performance slice compares idle and busy output in the same isolated 32-pane session. The existing issue's CPU target is at most 12% of one logical CPU; this alone does not verify badge latency or rendered input response. Check real agent start/stop and needs-input updates separately, using the existing harness/channel fixtures without changing their production decision logic. If a reliable timing source is absent, report that requirement unmeasured.

Expand to 1/5/20/50 total sessions only after the runner proves its own workload and counts. Visible splits and CRT-on are separate runs. Use five alternating pairs before claiming a performance improvement. Baseline and candidate share the same workload and hardware.

### Correctness lane

Add app/src/vt/lab.rs behind cfg(test), registered from app/src/vt/mod.rs. Drive existing Term::new/advance/resize/row_into/cursor/mode through TD's boundary. Compile once with each core; no production feature or new runtime. Reuse frozen TD flags and existing gridwire fixtures, but compare explicit state, not hashes alone.

Initial interface shapes:

- Case { format_version, corpus_version, name, seed, size, operations: Vec<Op> }
- Op = Write { bytes } | Resize { columns, rows } | Checkpoint
- Observation { case, checkpoint, cells, cursor, shared_modes, history, failure: Option<Failure> }
- Failure = StateMismatch | ChunkMismatch | Crash | TimedOut | SetupFailed | Unsupported { reason }
- replay(case: &Case, chunks: ChunkPlan) -> Result<Vec<Observation>, Failure>

Cell state uses TD-owned character/colour/attribute values; preserve wide-cell and wrap structure. Limit history, grid size, input bytes, cases and execution time before feeding a parser. Async synchronized-update deadlines are outside the first shared-semantics corpus; do not turn clock-dependent results into deterministic comparisons.

Create scripts/lab/run.py as the stdlib orchestration/replay/minimization entry. It generates a versioned corpus, builds/reuses the two test binaries and compares checkpoint observations. Chunk splitting must leave observations unchanged within each core. Cross-core disagreement is evidence requiring classification, not proof that either engine is correct.

Initial named cases cover printable text, basic colour/erase, cursor movement, alternate-screen entry/exit, wide characters and bounded resize. Add explicit expected state where feasible. Engine-specific semantics are named unsupported/known differences; never drop an inconvenient result silently. Truncated/malformed byte streams can expand the corpus after the tracer proves comparison and minimization.

Minimize operations, then Write payloads, preserving the same failure class and checkpoint meaning. Retain a small reproducer and exact replay command. Controlled mutations must demonstrate detection; a normal all-pass corpus is insufficient.

### GitHub lane

Create .github/workflows/td-lab.yml. Begin workflow_dispatch on trusted main with fixed seeds and four bounded shards, max-parallel no greater than four, timeout-minutes 15 including checkout/setup/build. Weekly scheduling is enabled only after a real manual run completes within budget and its mutation control proves detection.

Use existing pinned Rust and patched-GPUI preparation; caches include runner OS/image, core and toolchain. A cold build that misses the timeout is an infrastructure failure. Do not silently lengthen the agreed budget: first share/prebuild test binaries within the same run or reduce work; report any unresolved cold-build limit. A prebuild job replaces one of the four budgeted jobs; it is not an additional fifth job.

Each shard writes a concise summary and at most 10 MB of failure artifacts with seven-day retention. Successful runs retain summary/provenance rather than large raw dumps. Include missing/unsupported counts. No automatic issue creation in the initial pilot: add a read-only failure fingerprint first, then a separate minimal-permission publisher once actionable failures exist.

Add PR-only CI cancellation as the ancillary configuration change already proposed: workflow-scoped concurrency groups keyed by PR number; unique run groups for main and tags. Preserve existing checks and production dependency pins.

## Least confident decisions

1. Whether both core test binaries fit the 15-minute cold-run budget. Measure one real manual run before enabling the weekly schedule.
2. The first shared-semantic state normalizer. Existing gridwire fixtures help; snapshot serialization is not itself the correctness oracle.
3. Whether current event fixtures can prove a one-second background badge target without flaky wall-clock assertions. If not, expose the missing measurement and design the smallest specific instrument before changing production behaviour.

## Slices

1. Performance tracer: preserve and validate today's controlled run; make one invocation reproduce the same workload, raw records and human-readable result. Verify cleanup on error.
2. Performance coverage: validate background state responsiveness, then add pane-count matrix and explicit unavailable presentation metrics. Tune production only if a measured failure survives revalidation.
3. Correctness tracer: one named case, both cores, a checkpoint diff, exact replay and a controlled mutation reduced to a smaller reproducer.
4. Bounded corpus: versioned deterministic seeds, byte-chunk invariance, limits and named semantic exclusions; minimize real failures into fixtures.
5. GitHub pilot: manual bounded shards, failure artifact cap, summary and PR cancellation. Enable weekly dispatch only after the manual run proves budget and detection.

One task, one PR for this lab work. Keep the application's existing VT semantics and host protocol intact. Do not deploy or restart Parker's live window merely to benchmark.

## Validation

Per slice: relevant Rust tests on both cores, Python stdlib unit tests for real failure cases (missing outputs, malformed observations, timeouts, size caps, determinism and shrinking), and mutation checks. Before review: cargo fmt --check, both strict Clippy runs, both Rust suites, and workflow validation appropriate to the installed tools.

A lab detects a deliberate fault, minimizes it, and replays the same fault before it counts as working. A passing run with no measured input is setup failure.

## Approval

The priority order and budget are already approved. This combined gate asks only for the concrete implementation scope above. Recommendation: approve the measurement runner and correctness pilot in these slices; retain the conditional schedule and issue publication.
