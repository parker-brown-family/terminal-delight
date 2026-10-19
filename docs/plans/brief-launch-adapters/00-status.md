# Status: Shared launch briefing

**Difficulty: 5/10** — launch instructions and local note delivery cross the launcher and foreground detection, with shell argument handling to verify. One combined plan.
**Approval:** Parker said "Let's do it!" on 2026-10-14 in response to the First-launch briefs decision page recommending path A, brief every launch. This implements that approved path; it does not install global integrations or the alternative TD HTML renderer.
**Turned out to be: 5/10.** The local program needed explicit foreground identity as well as instructions; the existing WireMode::Other and notes routes carried it without a protocol change.

- Combined plan: APPROVED 2026-10-14 by the instruction above; routine adapter choices recorded in plan.md.
- [x] Tracer: Codex receives the shared packet through its supported initial prompt; Claude keeps its append-system-prompt channel.
- [x] Local: configured argv adapter receives the packet; the foreground program carries a scoped local-agent marker recognized by the notes route.
- [x] Integration: UI reports configuration and briefing errors; unsupported model/effort/reach dials are not offered for local programs.
- [x] Verification: shell capture with a temporary home, malformed configuration, note routing, format/clippy/tests, human-readable result.

## Verification

Seven new brief_launch regressions passed, including a real PTY local-to-shell identity and composer/prompt note delivery. Default suite: 2,242 passed, eight existing ignores. Formatting and clippy passed for both terminal cores. Fallback suite result is recorded in the final PR. Outcome page: reports/2026-10-14-shared-launch-briefing.html; all four figures and the decision accept notes, map export and saved-note embedding pass, no overflow at 1100/720/420px. No real-model compliance or live desktop cutover is claimed; the existing real-agent briefing follow-up remains open.

Working branch: feat/brief-launch-adapters, isolated worktree .worktrees/brief-launch-adapters. No unrelated root working-tree edits are included. Public release publication is outside the implementation approval; an updated release still needs to distribute this work.
