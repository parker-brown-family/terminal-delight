# Status: Hidden pane housekeeping

Difficulty: 5/10 — removing routine callbacks risks delaying real pane events. One combined plan.

Approved direction: Parker said "do that" after the cost/benefit assessment and explicitly authorized autonomous implementation, adversarial tests, rebuild and installation. No code review or further approval handoff is requested.

- [x] Tracer: remove local foreground watcher from host-owned panes; retain local detection.
- [x] Hidden housekeeping: one-second fallback, coalesced visibility/resize wakeups, explicit deadlines, hidden visual changes through repaint.
- [ ] Behavioral and mutation tests; both terminal cores; matched production CPU measurements.
- [ ] One merged PR, rebuilt production binary, isolated smoke and guarded live GUI reconnect.
- [ ] Human report with measured savings and limits.

## Current verification

Initial targeted run: seven real-pane tests passed on the fa88810 application plus the timer changes. The current test source separates the foreground-watcher oracle into an eighth test and adds no new behavior. The branch has since rebased cleanly onto Terminal State merge bfd7a71. Child-module visibility compile errors from the first attempt were fixed; both initial logs are retained.

Benchmark preflight now passes with two floated1000x700 windows on one private hidden workspace and monitor, each containing one visible quiet pane and thirty host-classified synthetic Codex workers. Failed preflights rejected a missing theme id, exec-replaced shell identity, incomplete startup and unequal monitor geometry before any CPU sample. The production baseline binary is currently td-55545af-terminal-state; use the same baseline binary as both fixture hosts. Old live-host compatibility is tested separately.

Quiet samples must precede working samples: pausing output leaves a working footer behind, and synthesizing stop events would play thirty completion bells. Both builds run simultaneously, five10-second pairs per condition, and every interval checks process start times, output counts and unchanged matching compositor geometry. No final CPU observations yet.

Native smoke script exercises a two-tab isolated session with host3678's executable, sends Ctrl+PageUp only to its own hidden GUI, saves the selected-tab proof, photographs the newly activated streaming pane, opens/scrolls a document, then reconnects the GUI and compares both pane identities.

Follow-up found during fixture validation: exec-replaced agents can retain SHELL classification because their foreground group still equals the original shell PID. Revalidate before fixing: https://github.com/parker-brown-family/terminal-delight/issues/991

Shared build target is currently held by painter pane110/thread01a14d48-9d7f-7170-b360-83ab21001a31 for fallback tests and release. Wait for its release notice; it will also merge its own palette changes before our final full checks. Terminal State owner is done: GUI2385158, host3678 unchanged. Do not touch root usage.rs or restart the host.

Next: rebase latest main if palette merged; run all eight current tests, the five production-mutation variants in reports/2026-10-17-pane-sleep-evidence/mutate.py, full required checks on both cores, a normal production build, matched CPU experiment, single PR and safe installation. Mutation script always byte-restores each changed source in finally. Never benchmark a test-feature build.

## Palette integration

Current base is painter commit0bbfd51 (its PR992 CI pending at integration time). The palette change removes the Gamba object and clock entirely, replacing its working-state storage with agent_thinking:Option<bool>. Resolved the two clock conflicts by retaining the removal. Our deadline requires Some(false); None remains unobserved. Removed the obsolete reel-repaint test/mutation, replacing that fault control with a bypassed scroll-settle gate. The seven current behavioral tests passed on this combined tree, including visible/unfocused CRT cadence and unknown-state rejection (integrated-targeted.log).

The shared target is now ours. Production mutations are running via background job shell_0a36ec63244a311c; do not edit/stage Rust while it temporarily holds deliberately broken source. Restore is automatic per case. Native smoke script itself passed against the baseline and the old host; rerun against our normal production candidate later. The matched baseline is now immutable td-0bbfd51-palette-rules, so reel removal will not be counted as a gain from this timer change.

## Verified application before matched builds

Default full suite:2412 passed,0 failed,9 ignored. Fallback:2365 passed,0 failed,9 ignored. Strict Clippy on both cores, formatting, cargo-deny and browser-prototype JavaScript syntax passed. validation.json records native_smoke:null until our own production candidate is exercised.

Five production fault variants failed their intended housekeeping assertions and byte-restored their source. A separate F1-routing fault now has a passing unmodified baseline and fails with the specific key-routing assertion. Its first attempt was invalid because the existing source reader was already failing; that rejected control and its classification are retained.

The first full run failed22 source guards because the new external test module was declared at the top, plus the known unchanged host identity test. The second run fixed the top-level module placement and found one remaining guard cutting at an indented cfg attribute. Eight source readers now use the existing top-level-boundary convention; all assertions remain intact. The final two core suites above passed. The unchanged host test also passed alone; no claim is made that its inferred fork/exec cause was confirmed or fixed.

Rebased onto merged main2b4a6b3 with SHA256 equality of all five changed application/test files before and after rebase. Main contains INHERIT, Terminal State and the palette/reel removal. Original autostash5393764 remains a recovery backup from pre-palette integration; do not apply it onto this code.

Next build baseline2b4a6b3 and the committed candidate sequentially in this same worktree path, using normal cargo build --release --locked. Keep both immutable binaries; do not switch the live launcher until measured/tested/merged. Then run two identical-binary control pairs per condition, followed by five baseline/candidate pairs, with no build/profiler active. Working samples also verify each hidden window's parsed output counter, not just its writers.

Current live GUI2866255, host3678, own pane98/shell1253854. Painter has finished and released builds/reconnect; no other shared-target work is planned.

Turned out to be: pending.
