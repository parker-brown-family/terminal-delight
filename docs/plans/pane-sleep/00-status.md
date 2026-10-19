# Status: Hidden pane housekeeping

Difficulty: 5/10 — removing routine callbacks risks delaying real pane events. One combined plan.

Approved direction: Parker said "do that" after the cost/benefit assessment and explicitly authorized autonomous implementation, adversarial tests, rebuild and installation. No code review or further approval handoff is requested.

- [x] Remove the local foreground watcher from hosted replicas; retain local PTY detection.
- [x] Hidden housekeeping: one-second fallback, bounded/coalesced visibility and resize wakeups, explicit resize and completion deadlines.
- [x] Seven behavioral tests, five production mutation controls and one qualified routing-guard control.
- [x] Full suites and strict Clippy on both cores, formatting, cargo-deny and JavaScript syntax.
- [x] Normal production baseline and candidate built from the same checkout path.
- [x] Identical-binary control and two complete matched experiments with opposite launch order.
- [x] Candidate native smoke against the old host executable: output, activation, document, GUI-only restart and both terminal identities.
- [ ] Merge the single PR and perform a guarded live GUI-only reconnect; never restart host3678.
- [ ] Deliver the final human report with actual live identity preservation proof.

## Verification and limits

Default full suite:2412 passed,0 failed,9 ignored. Fallback:2365 passed,0 failed,9 ignored. validation.json records checks and raw log hashes. Seven real-pane tests cover inactive/hidden cadence, visible and CRT cadence, hosted/local watchers, output/colour replies, prompts, latest-only settled resize, completion deadlines and scroll-settle. Each production fault fails the intended behavioral assertion; source bytes are restored.

The initial full runs exposed source-reader cutoffs caused by the new test module and indented test counters. The module now follows the established bottom placement; eight readers use the top-level boundary without weakening assertions. A passing unmodified routing guard rejects a deliberately inserted F1 bypass at the specific routing assertion. Its original invalid control is explicitly classified and retained. The unchanged host identity test also failed once, passed alone and in both final full suites; its inferred fork/exec explanation remains unconfirmed.

Matched production measurements against main2b4a6b3, which already includes reel removal, INHERIT and Terminal State: quiet CPU medians3.7% and2.8% lower; working0.4% lower then1.8% higher. Identical-binary controls differed by up to3.3 points of one CPU, larger than the1.0–1.4 quiet median savings. All20 A/B pairs plus4 control observations are retained. Each working observation checks fresh parsed output in all30 hidden panes, matched writer rates, executable/process identity and matching1000x700 geometry. No CPU-based exclusions. CPU benefit remains small/uncertain; whole-machine speed, battery and input latency are unmeasured.

Normal candidate td-6736bc7-pane-sleep contains the exact tested application. Its separate older-host native smoke passed. Source app bytes remain unchanged after those checks. Human candidate reading: reports/2026-10-18-pane-sleep-preview.html. Final delivery adds actual reconnect proof; the preview does not claim installation.

## Independent follow-ups and ownership

Exec-replaced fixture agents exposed a separate host classification assumption: https://github.com/parker-brown-family/terminal-delight/issues/991
Existing intermittent identity test record: https://github.com/parker-brown-family/terminal-delight/issues/933#issuecomment-6246691310
Broader processor investigation remains on issue964; this timer change does not close it.

All work is in the isolated perf/pane-sleep worktree. Root usage.rs is unrelated and remains untouched. Palette owner released the shared target and GUI slot. Before cutover use a fresh census and the archived guarded reload procedure; host3678 must remain running.

Implementation calibration:5/10. The deadline semantics, source-guard controls, integration with reel removal and uncertain CPU effect justified the original score.
