# 0.5.0: Agent-to-Agent Communication Hardening

**Target:** Friday, November 20, 2026 (America/Vancouver).
**Candidate target:** Friday, November 13, 2026.
**Development branch:** `dev/0.5.0`.
**Version bump:** prepare the candidate version through the existing release process; this branch setup changes no package version.
**Difficulty:** 2/10 for release setup; the underlying management feature remains the approved 8/10 design. No new feature gates are opened by recording this release.

Parker selected November 20 and made hardening agent-to-agent communication the purpose of 0.5.0. Use the feature locally, preserve the evidence, and integrate through this development branch while 0.4.0 finishes. The date is a target; a failed qualification or restarted candidate hold moves it.

## What This Release Must Establish

A managing agent can assign work to an already-running agent and inspect what happened afterward. Each request must identify its original sender and recipient, preserve native delivery and consumption evidence, and distinguish a recipient’s result from a manager’s verification. A timeout must not silently create a second assignment.

Harden the existing design in these areas:

- **Correct recipient:** declared conversation/process binding, live process and native-endpoint generation, supported harness profile, and correct per-thread tool routing.
- **Useful records:** model requested and reported, harness/runtime versions, process identities, parent/cause relationships and source timestamps. Unknown and uncollected values remain explicit.
- **Honest handoffs:** durable confirmation, native acceptance, consumption, recipient reports and manager dispositions retain separate meanings.
- **Recovery and control:** replay, crash/reopen, deadlines, cancellation, queue credit and store failures preserve the approved behavior.
- **Daily use:** repeatable public-MCP exercises across multiple existing recipient processes, with sanitized exports and reproducible failures.

The existing exposure and writes controls remain in force. Forwarded original-agent identity is a cooperative assertion and is recorded that way. This release does not add automatic agent creation, resume, forks, PTY input or process termination.

## Schedule

| Dates | Planned Work | Evidence Needed |
|---|---|---|
| October 20–30 | Use the development build locally; establish supported native profiles and identity facts. | Exact runs and version/model provenance; unsupported states remain visible. |
| November 2–6 | Exercise recovery, ambiguous delivery, queue credit and failure boundaries. | Deterministic checks and controlled live recovery exports. |
| November 9–12 | Finish qualification and prepare the candidate. | Qualification report, green checks and known limits. |
| November 13 | Target the first candidate. | Tested artifact, digest and release checklist. |
| November 13–20 | Hold and use the candidate for at least seven days. | Daily observations; fixes restart the hold for touched behavior. |
| November 20 | Target stable 0.5.0 when the hold and checklist pass. | Evidence for the candidate actually promoted. |

November 20 is Parker’s explicit Friday exception to the usual Monday schedule in [RELEASING.md](../../../RELEASING.md). The seven-day candidate hold is retained.

## Work and Current Evidence

- [Milestone: 0.5.0 agent-to-agent communication hardening](https://github.com/parker-brown-family/terminal-delight/milestone/2).
- [Recorded native MCP handoffs](https://github.com/parker-brown-family/terminal-delight/pull/1069): implemented and locally tested; draft integration PR.
- [Multi-pane qualification and candidate hold](https://github.com/parker-brown-family/terminal-delight/issues/1071): the measurable release work, including 50 harmless handoffs across at least two existing recipients and three controlled window-recovery exercises. Coverage is uncollected until linked evidence exists.
- [Shared Codex daemon routing](https://github.com/parker-brown-family/terminal-delight/issues/1059): existing dependency; prove binding and routing before admitting that native profile. No duplicate defect is created here.

The implementation has small successful Claude native and public-MCP demonstrations, plus passing local tests for both terminal cores. Those results do not establish sustained multi-pane qualification. The active released window still lacks the new management operations; local experimental use must identify the development build actually running.

## Branch and Release Rules

The branch starts from the current `origin/main` baseline, before the final 0.4.0 release snapshot is settled. Bring the final 0.4.0 changes forward through a reviewed integration before cutting the 0.5.0 candidate. Keep one feature PR for the management implementation and target it here.

The branch identifies local development. Package version changes stay with candidate preparation, as required by the release policy; a development branch is not a release tag. Publishing still follows [RELEASING.md](../../../RELEASING.md): only a tested snapshot integrated through `main` or `release/*`, with green checks and the matching candidate/stable tag, is published. No current 0.4.0 release tag, asset or default installation is changed by this setup.
