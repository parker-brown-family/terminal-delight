# Status: Artifact continuity and explicit pane closure

**Difficulty: 7/10** — persistent review evidence and process ownership survive restarts.
**Turned out to be: 7/10.** Review evidence, process identity and a live host upgrade each required separate verification; the original score matched the work.

Parker authorized the recommendations on 2026-10-24: “Do all the work now.” He then explicitly included the work in 0.5.0, requested detailed changelogs and asked for the current TD session to run it. Implementation is original TD code informed by the research; no HumanLayer source or translated implementation is to be copied.

The research page supplied the product direction. The remaining design is recorded before implementation under that authorization; no additional gate or slice confirmation is requested.

- Product: selected from the annotated research and current instruction.
- Architecture: recorded in 02-architecture.md.
- Program design: recorded in 03-program-design.md.
- Slices: recorded in 04-slices.md.

## Slices
- [x] Bounded explicit terminal closure, owned-process signaling and completion evidence.
- [x] Durable artifact lookup and observed local-file revisions using existing records.
- [x] Markdown review quotation and revision evidence with ambiguous/outdated outcomes.
- [x] Integration, native verification, detailed changelogs and one PR targeting dev/0.5.0.
- [x] Preserve current running-session features, install the qualified build, verify live capture and review behavior.

## Boundaries
Existing terminal host independence, permission controls, note data and handover history remain authoritative. No automatic agent creation, remote teleport, whole-platform adoption or live-host FD migration is in this implementation. A GUI upgrade cannot silently replace the host that owns running PTYs. Record the actual live boundary in installation evidence.

## Implementation

The isolated feature checkout is `/home/parker/Work/td-authority-lab/humanlayer-continuity`; the daily integration checkout is `../td-humanlayer-integrated`, based on the current War Room and MCP integration. The original working tree and its usage edits were left untouched.

The archive uses existing conversation records, with explicit unbound events before conversation identity is available. Capture runs off the UI thread. Markdown notes capture their quotation, neighbouring passages and source hash when created; the version-two store preserves legacy notes without inventing missing evidence. Explicit close runs at the PTY owner, uses retained child identity and pidfds, and reports incomplete inspection or unobserved exits.

## Qualification Update

The default-core full suite passed: 2,707 unit tests, 116 integration tests, and 17 deliberately ignored tests across the binaries. Commands used `cargo test --locked -- --test-threads=4` with a separate XDG state root. Native tests wrote a Markdown note through its rendered control, moved the passage, reopened the file and verified the original quote/hash; a second native test ended a stubborn window-owned child and observed its reaping. Host socket and real Chromium integration tests passed.

An earlier saturated run found a process-scan timeout in the test setup. Inspection exposed a real improvement: an incomplete scan now retains identities already acquired, signals those jobs, and leaves the total remaining count unknown. Each scan has a 100 ms budget. The same run reproduced the existing erased-trail test failure recorded in issue 1126; that unchanged test passed alone, and the complete four-thread run passed. This is not counted as a repaired linger defect.

The six-figure review has working note and concurrence export, saved HTML islands, and reload persistence. Its narrative check classified it as a journey, with cold-opening probability 0.90. Layout was observed without horizontal overflow at 1,408 and 520 CSS pixels; requested narrower browser overrides did not reliably take effect. The fallback suite also passed: 2,654 unit tests and 116 integration tests, with 17 ignored. The five restricted-command OS tests were then run explicitly and passed. Both strict Clippy runs and the optimized feature build passed. The final daily integration, including the newer sticky-note stamp and corrected browser route, passed 2,724 unit and 116 integration tests plus strict Clippy. Its optimized executable passed the native capture/note fixture and is installed in the current window.

## Final Interaction Correction

The optimized fixture proved repeat handover capture and moved-note recovery. Its pointer controller could not activate the Archive button in a hidden window. Source inspection then identified a separate product mismatch: the native HTML reader does not forward input events to form fields. Archive now opens the local search page through the existing system browser opener. The browser check exercises the exact generated template with fixture rows; Rust tests cover record loading, integrity and HTML escaping. No general document-input bridge was added.

## Installed Development Build

The guarded swap installed `td-c12e07b6-0.5-artifact-continuity`, SHA-256 `b5c0f33706b233ea648a7f9448c26f50f5fc624731402f114652f5c572bbcf3f`, as GUI 357460. All 36 live host terminal identities, 32 exposed panes, persistent layout, MCP policy, window geometry and the saved split document were preserved. No panes closed. The prior launcher was `td-1cb31700-0.5-dev-note-stamp`; its robot stamps are included in the combined build. The first swap attempt refused before changing anything because that newer build had arrived during qualification.

Host 2108452 still runs `td-babe2dc-translations`. The window update activates archive and review changes; hosted cleanup requires a later host restart. Existing live-host migration work remains outside this change. No stable tag or package version was published.

The single review unit is [artifact continuity and bounded cleanup](https://github.com/parker-brown-family/terminal-delight/pull/1141), targeting `dev/0.5.0` and the 0.5.0 milestone. Its checks and merge state are authoritative. Receipts are under `reports/2026-10-24-td-continuity-evidence/`; complete local build and test logs are under `/home/parker/Work/td-authority-lab/humanlayer-qualification/`.
