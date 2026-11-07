# Status: the entry point (INBOX spine + chat launcher)

**Difficulty: 6/10** — two new surfaces on a strict window, nine languages, persisted person
state; both shapes exist already and the branch is unmerged → one gate (`plan.md`).
**Turned out to be:** _filled in at the end_

- Gate — one-page plan: APPROVAL WAIVED 2026-11-07 (Parker: "build build build … I am fully
  afk"; recorded as a judgement call in the brief)

Branch: `explore/t3-scuttle`, cut from `origin/dev/0.5.1` at `4b184dc0`. **Not for a launch
branch.** Nothing here merges into `dev/0.5.1` or `main` without Parker.

Build container: `~/Work/td-t3-scuttle/{terminal-delight,zed-upstream,target}` — a private
gpui checkout with the patch stack through 0014, so this branch never builds against the
follower's tree (`~/Work/td-consolidate-lab`).

## Slices

- [ ] Slice 1 — tracer bullet: `inbox.rs` partition + tests; PROJECTS │ INBOX toggle; rows focus
  their pane
- [ ] Slice 2 — the verbs: snooze, done, wake, reopen; persisted store; early wake; WOKE
- [ ] Slice 3 — the composer: draft, chips, Enter / Ctrl+Enter launch with the sentence as opener
- [ ] Slice 4 — keyboard: Ctrl+Shift+I, Ctrl+Shift+E, keys inside both surfaces
- [ ] Slice 5 — the farbox: `machines.toml`, machine chip, gliff action
- [ ] Slice 6 — nine languages, gates, one Codex review round, photographs, brief

## Notes for a fresh session

- Research inputs: Theo's video transcript (YouTube `uLGxb4F3SyI`, same cut as the X post),
  T3 Code source at `pingdotgg/t3code` e4552000 (`packages/client-runtime/src/state/threadInbox.ts`,
  `apps/web/src/components/Sidebar.tsx`, `docs/user/thread-sidebar.md`), the 554-ADE intent audit
  in `~/Downloads/ADE_Intent_Audit_554.csv`, and `gliff` 0.4.0 (`/usr/share/doc/gliff/README.md`).
- The shelves and their names are Parker's, from his mid-run message: LOOK AT ME on top, then
  SLEEP & QUIET, DONE, and snooze.
