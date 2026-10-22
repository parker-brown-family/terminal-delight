# Process authority: implementation and qualification

**Difficulty: 8/10.** Enforcing different authority for programs sharing one owner crosses launch, local control, lifecycle and delegation. **Actual: 8/10.** OS probes exposed descriptor inheritance and later-created sockets, both requiring changes to the boundary.

Parker authorized autonomous completion on 2026-10-21: “Great - execute now! The other agent is done and you have the floor 100%... I am afk - so get it end to end including automated testing etc.” This supersedes the earlier planning hold. It authorizes design and execution without separate gate replies; it is not recorded as reviews of documents Parker has not read.

## Selected state

- Product: preserve arbitrary Standard terminals; add an explicit offline Restricted command profile. See [the product](01-product.md).
- Architecture: OS containment before workload startup, one chosen directory, fresh environment and plain-text streams. See [the boundary](02-architecture.md).
- Program design: preconnected typed own-note channel, immutable optional grant, exact process/start-time binding, external revocation and no automatic handoff. See [the contract](03-program-design.md).
- Slices: implemented and under final qualification. See [verification](04-slices.md) and [the readable review](../../../reports/2026-10-21-restricted-commands.html).

The first [security implementation](https://github.com/parker-brown-family/terminal-delight/pull/1088) and the [review hardening](https://github.com/parker-brown-family/terminal-delight/pull/1095) are merged into `dev/0.5.0`. The remaining review completion is one follow-up PR on `fix/restricted-review-completion`. The 0.4 release, tags and assets are outside this change. The original launch/access mockups and research are historical design inputs; the implemented surface is a command, not a launch GUI.

## What changed

All six global MCP policy mutations are refused in ordinary windows. Operator rigs must set `TD_MCP_SCRIPTED=1` before launching their own window. This prevents cooperative policy changes and nuisance self-disable; it does not contain hostile same-user Standard programs. The off-limits bench bypass is closed at selection, probe and dispatch, including synthetic hover/click and scripted face selection.

Restricted commands require bubblewrap 0.12.0 or newer, Linux descriptor cleanup and the enforced namespace/seccomp setup. Missing or unsupported setup prevents command execution. No Standard fallback exists. Workload descendants share that run's filesystem scope and optional note grant. Online agents, TUIs, GUI services, provider credentials and automatic handoffs are unavailable in this first profile.

The [broad authority issue](https://github.com/parker-brown-family/terminal-delight/issues/1087) and [off-limits pointer issue](https://github.com/parker-brown-family/terminal-delight/issues/1090) are addressed here. The separate [native-handoff implementation](https://github.com/parker-brown-family/terminal-delight/pull/1069) retains its cooperative Standard contract. Its [multi-pane qualification and candidate hold](https://github.com/parker-brown-family/terminal-delight/issues/1071) remain independent release work.

## Review completion (in progress)

[The completion decisions](05-review-completion.md) extend the four selected gates under Parker's explicit AFK delegation. The chosen directory is a non-hidden home descendant and is validated through pinned descriptors and mount identity. Peer credentials now establish own-pane authority; remaining config, document, tab and queued-input paths have regression coverage. A single proposal appears in the run terminal with outside-only dismissal. Development pushes run CI; the dedicated user guide is linked in the site and feature catalog.

The post-merge inspection found that the surface patch still persisted before Apply: a stale allowed snapshot could leave a refused card for a later sweep. Persistence now follows an accepted UI receipt, and the test includes an allowed snapshot whose live Apply refuses. The actual effect also rechecks the master and Writes policy, preserving asks with Writes off.

No live installation is authorized this round. All native qualification uses independent scratch sessions. Original receipts are historical and must not be presented as current-code checks. Current full checks, mutation results, source/binary digests and passed CI will be recorded in the updated readable report. The private media rig now refuses ctl/RPC failures (local commit a4b6e22).

## Limits

Standard processes retain normal account authority. Restricted output, notes and shared files can mislead a later unrestricted reader; TD does not claim to constrain that reader. Secrets already in the chosen work directory are granted content. Complete resource quotas, hostile administrators and kernel exploits are outside this qualification. Revocation blocks future TD effects after any in-flight note completes; it does not undo files or stop ongoing file work.
