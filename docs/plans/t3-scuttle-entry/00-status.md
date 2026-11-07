# Status: the entry point (INBOX spine + chat launcher)

**Difficulty: 6/10** — two new surfaces on a strict window, nine languages, persisted person
state; both shapes exist already and the branch is unmerged → one gate (`plan.md`).
**Turned out to be:** a 6, and it bit where the score said it would: in the window's own rules
rather than in the new modules. The pure models came out right first time; every real defect
found later (the queue answering keys behind the composer, a pane behind a page counted as
visited, a timer booked every frame, the inbox computed before the reap) sat where a new
surface meets the window's keyboard, focus and frame rules.

- Gate — one-page plan: APPROVAL WAIVED 2026-11-07 (Parker: "build build build … I am fully
  afk"; recorded as a judgement call in `entry.calls.json`)

Branch: `explore/t3-scuttle`, cut from `origin/dev/0.5.1` at `4b184dc0`. **Not for a launch
branch.** Nothing here merges into `dev/0.5.1` or `main` without Parker.

Build container: `~/Work/td-t3-scuttle/{terminal-delight,zed-upstream,target}` — a private
gpui checkout with the patch stack through 0014, so this branch never builds against the
follower's tree (`~/Work/td-consolidate-lab`).

## Slices

- [x] Slice 1 — tracer bullet: `inbox.rs` partition + tests; PROJECTS │ INBOX toggle; rows focus
  their pane
- [x] Slice 2 — the verbs: snooze, done, wake, reopen; persisted store; early wake; WOKE
- [x] Slice 3 — the composer: draft, chips, Enter / Ctrl+Enter launch with the sentence as opener
- [x] Slice 4 — keyboard: Ctrl+Shift+I, Ctrl+Shift+E, Ctrl+Alt+arrows and digits on the INBOX
  face, the composer's keys
- [x] Slice 5 — the farbox: `machines.toml`, machine chip, gliff action, the line previewed
- [x] Slice 6 — nine languages, gates, one Codex review round (4 findings, all fixed), photographs
  from a private display, the brief

## Evidence

- Gates on the final code: `cargo fmt --check`, `cargo clippy --locked -- -D warnings` (CI's), the
  full suite twice (second run: 3,307 unit + 124 integration, 0 failed). 48 tests in the four new
  modules.
- Review: one read-only `codex exec` round. Work order and reply are in the brief's evidence
  folder (below), not in this public repository.
- Brief (local, untracked, with its evidence):
  `~/Work/terminal-delight/reports/2026-11-07-t3-scuttle.html`

## Open

- **LOOK AT ME's order is Parker's call** (the one call `brief.py` stopped on): the rail's order
  (default) or T3's newest-return-first, one press apart under the shelf's header.
- No auto-settle (T3 settles after days idle or a merged PR); no undo after a verb; no bulk
  select. Each is small on top of `inbox::Store`.
- Host pane ids restart if the session host restarts, so a stale mark could land on a new pane
  with a reused id; `FORGET_AFTER` is the only guard.
- A far agent's state is never read; its row says unknown. Spinning TD itself up on the farbox
  inside gliff's private output needs a workspace rule on the far side and was not attempted.

## Notes for a fresh session

- Research inputs: Theo's video transcript (YouTube `uLGxb4F3SyI`, same cut as the X post),
  T3 Code source at `pingdotgg/t3code` e4552000 (`packages/client-runtime/src/state/threadInbox.ts`,
  `apps/web/src/components/Sidebar.tsx`, `docs/user/thread-sidebar.md`), the 554-ADE intent audit
  in `~/Downloads/ADE_Intent_Audit_554.csv`, and `gliff` 0.4.0 (`/usr/share/doc/gliff/README.md`).
- The shelves and their names are Parker's, from his mid-run message: LOOK AT ME on top, then
  SLEEP & QUIET, DONE, and snooze.
- To look at it: `TD_SESSION=t3 ~/Work/td-t3-scuttle/target/release/terminal-delight`, plus
  `TD_INBOX_DEMO=1` for a filled inbox. The screenshot rig (`rig.sh`, `shoot.sh`, stand-in agents
  named `claude` that call no model) is in the brief's evidence folder. A stand-in must paint
  AFTER the window has classified its pane, and keep repainting, or its question is never read.
- Never launch a real agent from a rig: the composer's claude line runs the real Claude Code. The
  rig sends through `local-agent.toml` to a stand-in.
