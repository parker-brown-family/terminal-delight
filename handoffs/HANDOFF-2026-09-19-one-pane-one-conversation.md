# Handoff — one pane, one conversation (2026-09-19)

## Status

Landed. **PR #576** (the resolver) and **PR #581** (a pane follows the keyboard) are merged into
`main`; **PR #589** (documentation: the falsification table) is open and green locally. Installed
build `td-569f452-allin` carries both merges — Parker's running window predates them, so a relaunch
is the last step. My worktree `~/Work/td-resolver` is clean and on `docs/the-guards-say-how-to-break-them`.

## What's done

- **`app/src/paneident.rs` is the single answer** to "which conversation is this pane in". It binds a
  whole window in one pass over `vitals::assign_bonded`, labels each binding
  `Declared | Birth | Sole | Guess`, and returns nothing where the evidence stops. The bench, tool
  glyph, `pane_events`, the desktop recap and the agent wall call `paneident::certain()` and act only
  on the three evidenced rungs. *Verified:* 1371 unit tests; four guards broken on purpose and
  confirmed red.
- **The per-pane resolvers are deleted** — `session::claude_transcript` and
  `mcp_tail::transcript_for` both ended in newest-file-in-the-directory, which is one answer for
  every pane sharing a folder. A source scan in `paneident`'s tests fails the build on their return.
  *Verified:* mutation — adding the string back to `toolprop.rs` turns the scan red.
- **Two exact rungs added:** the `/tmp/claude-<uid>/<slug>/<session-id>/` scratchpad descriptor, and
  the SessionStart ledger (which the agent wall was not consulting). *Verified live:* 38 of 43 agents
  now bind `declared`.
- **`agent_under` asks the terminal's foreground process group** rather than taking the oldest agent
  child, so a pane with a suspended agent beside a live one follows the keyboard. *Verified:*
  measured on shell 1320940 — agent 1384250 stopped holding `cfa9eefd`, agent 1390176 foreground
  holding `470a6cfd`; the binding moved from `birth` (a coincidence) to `declared`.
- **The ledger's JSON reader tolerates whitespace** after the colon. It matched the literal
  `"session_id":"` and silently ignored any pretty-printed entry.
- **Machine-level repair, not in any PR:** `scripts/install-recovery-hook.sh` was run on this box,
  and ~40 live agents were bound by hand into `~/.local/state/terminal-delight/agent-ledger/`. That,
  not the merged code, is what is holding the live window's panes apart right now.

## How to run / verify

```
terminal-delight bindings
```
Every live agent, its conversation, and the rung that bound it, as JSON. No window required. Expect
no session `certain` for two different panes.

```
cd app && cargo fmt -- --check && cargo clippy --locked -- -D warnings && cargo test --bin terminal-delight
```
The three gates CI runs. Current: clean, clean, 1371 passed.

**The mutation recipe** (break the rule, expect the test to fail) is a table in the header of
`app/src/paneident.rs`, added by PR #589. Run it before trusting any of those guards again.

## Not done / next

- **#583** — a derived bench card outlives the binding that produced it. `deliver_surfaces`
  (`main.rs:7544`) only ever calls `present()`; nothing retires a post when a pane rebinds. This is
  the visible residue Parker saw, and it persists until dismissal or restart.
- **#584** — codex panes have no fleet pass, and `codex_rollout_for` matches a cwd as a **substring**,
  so a pane in `/home/parker` can select a rollout opened in `/home/parker/PROJECT`. PR #576 demotes
  such a pair to `guess`, which stops the wrong attribution but binds neither.
- **#579** — `instance::tests` flakes about 2 runs in 11 under full-suite parallelism, **on pristine
  main**. Not this work; it failed twice during it and reads as though it were.
- **Unconfirmed with Parker:** the agent wall now clears a card it cannot attribute rather than
  drawing probable bars. One line to reverse if he dislikes it.

## Watch out

- **Two branches in the resolver have no specimen** and are documented rather than hardened: the
  highest-descriptor tie-break when a process holds two scratchpads, and the first-child fallback
  when every agent under a pane is backgrounded. Zero of 44 agents hit either. A pane stuck on a
  conversation it has left is the symptom that sends you to the first.
- **A test's synthetic pid must be impossible, not merely free.** pid 4242 passed here and failed in
  CI. Use a value above `pid_max`.
- **Every wait in a test needs a deadline.** The push-feed test blocked forever on a channel and
  stalled a suite run at 1327 green tests with no verdict.
- **This repository merges fast from several sessions.** #576 was merged by somebody else at the
  commit before the last fix; that fix had to be cherry-picked onto main as #581. Check
  `git merge-base --is-ancestor <sha> origin/main` before assuming a push landed.
- `~/Work/terminal-delight` is a shared worktree with another session's uncommitted work in it. All
  of this was done in `~/Work/td-resolver`; nothing in the shared tree was touched except `reports/`
  and `handoffs/`.

## Where it's recorded

- APES episode: `…/apes/projects/terminal-delight/episodes/2026-09-19-one-pane-one-conversation.md`
- APES kanban: one ticket closed with deliverable; three follow-ups in `todo`, each cross-linked to
  its GitHub issue.
- lean-ctx: session decision recorded (resume breadcrumb).
- file-memory: `a-shared-cwd-collapses-every-pane-onto-one-transcript`,
  `measure-before-you-harden-a-branch`, `a-merged-pr-can-take-your-branch-without-your-last-commit`,
  and an addition to `a-metric-needs-a-leg-that-fails`.
- Session harvest: `handoffs/2026-09-19-one-pane-one-conversation.cdx`
- Briefs: `reports/2026-09-18-benches-showed-the-wrong-agent.html` (diagnosis),
  `reports/2026-09-18-one-pane-one-conversation.html` (the fix and what its tests prove).
