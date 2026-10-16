# Status: an agent asks on the rail, a person answers yes or no

**Difficulty: 6/10** — one combined plan (`plan.md`). The product decision is Parker's
(2026-10-15, on the Splinterm scuttle brief: *"decision made - the agent spine will now
allow requests to show up with a SIMPEL YES NO"*). The design choices are mine, built on
a branch, and the pull request is where they get read.

**Turned out to be:** 6/10. The pieces were all there (the rail, the pane-write pipeline,
the bench's signed line), and the work was in the guards: three existing ones had to learn
the new path rather than be bypassed — the off-limits writer list, the bench's write-site
scan (which is why the answer line waits for the pane to be on screen), and the spine
guide's citation check. One mutation survived the first test (a Yes on the wrong switch is
still "one switch moved") and the test was tightened.

- Branch: `feat/spine-asks-for-access`, worktree `.worktrees/spine-asks`.
- APES ticket: `an-agent-that-wants-an-access-setting-changed-asks-on-the-attention-rail-and-a-p-mvaa2muw`.

## Slices
- [x] Slice 1 — the ask end to end: `request_access` (mcp.rs), `access.rs` (the rules and
  the words), `AttentionKind::Access` and `AskRow` (attention.rs), the ask stored on the
  asking pane, the row with Yes / No and `y` / `n`, `rail_answer`, the answer line.
- [x] Slice 2 — docs and refusal text: `docs/mcp.md`, `docs/attention-spine.md` (its first
  rule amended for this one case), the docsite MCP page; the five "writes are disabled"
  refusals name `request_access`.
- [x] Slice 3 — PR 957, and the row photographed from this branch in a hidden window
  (`TD_SPINE_DEMO=1`, whose demo now includes an ask). Merging is Parker's.
