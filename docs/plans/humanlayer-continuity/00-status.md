# Status: Artifact continuity and explicit pane closure

**Difficulty: 7/10** — persistent review evidence and process ownership survive restarts.
**Turned out to be:** pending.

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
- [ ] Integration, native verification, detailed changelogs and one PR into dev/0.5.0.
- [ ] Preserve current running-session features, install the qualified build, verify live behavior.

## Boundaries
Existing terminal host independence, permission controls, note data and handover history remain authoritative. No automatic agent creation, remote teleport, whole-platform adoption or live-host FD migration is in this implementation. A GUI upgrade cannot silently replace the host that owns running PTYs. Record the actual live boundary in installation evidence.

## Implementation evidence so far

The original stubborn-child regression failed against the base: cleanup was still waiting after two seconds. The first implementation passes five real-process tests covering bounded close, a foreground job in a separate process group, a reaped child, weak PTY observation, disconnected clients, and stale close handles. Broader host/native checks remain pending. Closing hosted panes now runs away from the UI thread.

Artifact discovery is wired to the rail's Archive action. New handovers enqueue bounded local-file capture into the existing conversation records; unbound artifact surfaces have an explicitly unbound durable record. The generated archive has local search, current-target links and captured revision links. Tests and additional integrity checks are still in progress.

Markdown review evidence now captures the quotation, neighbouring passages and source hash when the edit is created, so a queued save does not substitute a later document. The version-two notes store preserves old notes without inventing their missing evidence. Reattachment and the UI/export paths are implemented; regression verification is pending.

The isolated checkout is `/home/parker/Work/td-authority-lab/humanlayer-continuity`, using the integration lab's qualified renderer. Its build cache is `/home/parker/Work/td-authority-lab/humanlayer-target`, a Btrfs copy-on-write clone of the existing cache. This avoids blocking the other agent building `td-install-integrated`. The feature is being prepared for one PR into dev/0.5.0. Installation is pending.

## Qualification Update

The default-core full suite passed: 2,707 unit tests, 116 integration tests, and 17 deliberately ignored tests across the binaries. Commands used `cargo test --locked -- --test-threads=4` with a separate XDG state root. Native tests wrote a Markdown note through its rendered control, moved the passage, reopened the file and verified the original quote/hash; a second native test ended a stubborn window-owned child and observed its reaping. Host socket and real Chromium integration tests passed.

An earlier saturated run found a process-scan timeout in the test setup. Inspection exposed a real improvement: an incomplete scan now retains identities already acquired, signals those jobs, and leaves the total remaining count unknown. Each scan has a 100 ms budget. The same run reproduced the existing erased-trail test failure recorded in issue 1126; that unchanged test passed alone, and the complete four-thread run passed. This is not counted as a repaired linger defect.

The six-figure review has working note and concurrence export, saved HTML islands, and reload persistence. Its narrative check classified it as a journey, with cold-opening probability 0.90. Layout was observed without horizontal overflow at 1,408 and 520 CSS pixels; requested narrower browser overrides did not reliably take effect. Fallback core, final lints, optimized build and installation are still in progress.
