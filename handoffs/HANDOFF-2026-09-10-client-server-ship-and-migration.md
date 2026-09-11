# Handoff — client-server split: shipped & migration (2026-09-10)

## Status
**MERGE-READY, merge NOT done (Parker's to click).** Branch `client-server-split`
@ `3ead832`, pushed, PR #328 **CLEAN + mergeable**. Hosted is the default; the
whole feature is one PR for one-revert rollback.

## What's done (this session)
- **Reversal shipped & verified.** Window-steal deleted, `hello` optional again
  (uid check `peer_is_us` is the boundary). Verified in code: `refuse_the_ungreeted`
  and `gui_conn` gone, ungreeted-connection test inverted, stand-down carve-out kept.
- **Main merged** (`5b47ce4`). Sole conflict was one `main.rs` region — our
  `TD_KEEPALIVE` typing loop vs main's deletion of that feature (#317). Took main's
  deletion. `keepalive.rs` gone, no dangling refs. **705 tests green** (was 724; the
  ~19 keepalive tests went with the feature). #314 dispatch allowlist intact.
- **Flip committed** (`bcb3a15`) as one revertable commit: hosted is the default
  cold-launch path; `TD_NO_SESSIOND=1` opts back to serverless; scratch/seed/demo
  never host; `spawn_in` untouched.
- **Harness fixed** (`3ead832`): the flip inverted the floor-control leg's assumption
  ("no flag = serverless"); serverless now uses `TD_NO_SESSIOND=1`.
- **Verified in isolation:** headless host smoke (host runs, persists across client
  connections, ungreeted verb answered, close-pane reaps the right process); flip GUI
  smoke (default launch hosts, `TD_NO_SESSIOND=1` serverless, no leak into real config).
- **Gates pass:** survival 20/20 kill-relaunch, realistic bench 30µs p99, 0/1000 over 1ms.
- **Migration prepped:** 115 session TOMLs snapshotted; new build proven to read
  old-format TOMLs; all 13 live-session resume lines present.

## How to run / verify
```
cd /home/parker/Work/td-client-server
cargo test --manifest-path app/Cargo.toml            # 705 pass
cargo build --manifest-path app/Cargo.toml           # -> app/target/debug/terminal-delight
node scripts/td-host-lab.mjs up                       # isolated host, no window
bash scripts/td-survival-test.sh                      # full survival metric (opens windows ~10 min)
/home/parker/bin/gh -R parker-brown-family/terminal-delight pr view 328 --json mergeable,mergeStateStatus
```

## Not done / next
- **The merge** — Parker's click. Then the **per-window migration** (gentle): install
  the new build via versioned-path + symlink-swap (never over the running binary),
  relaunch each window at a chosen moment. Get agents to a clean checkpoint first.
- **Follow-ups (GitHub, `follow-up` label):** #348 (comments overstate guarantees),
  #349 (scratch-cleanup regression assertion — run its own invalidation first), #352
  (three survival-harness legs). Deferred: close-undo held-state (post-flip).
- **Optional pre-migration test:** live-adoption dry-run — copy live `1.toml` into a
  scratch config, launch the new build, watch the 13 panes come back (one window).
- **Cleanup:** several stale worktrees (`cs/contract-reversal`, `cs/host-debts`,
  `cs/slice-3`, `cs/close-undo`) — prune post-merge.

## Watch out
- **Migration is a ONE-TIME RESPAWN.** Layout, cwds, agent resume lines and sticky
  notes survive (TOML the new build reads); **live processes and in-memory scrollback
  do not** — a new host can't inherit PTYs born under the old serverless window.
  After the first hosted relaunch, window death is survivable forever.
- **Scrollback is host-memory-only, never on disk** (by design) — reboot/host-crash
  loses it, not layout/resume. A detached session exits after ~12h idle (checkpoints first).
- **Rollback hatches:** `TD_NO_SESSIOND=1` (serverless, no revert) · symlink back to
  `td-95cec1d-main` · revert `bcb3a15` (flip) · revert the PR.
- **Never install over `~/.local/bin/terminal-delight`** (another agent's build, #327).
  Never build in `~/Work/terminal-delight` or the fork.
- Two windows on one session now **share** panes (steal removed).

## Where it's recorded
- **APES episode:** `.../apes/projects/terminal-delight/episodes/2026-09-10-client-server-ship-and-migration.md`
- **APES tickets:** `…-mtvymzun` (session, closed) + 4 feature tickets closed.
- **lean-ctx:** `ctx_session` decision recorded (this session).
- **file-memory:** `client-server-split.md` updated to the shipped state.
- **Harvest:** `handoffs/2026-09-10-client-server-migration-tieoff.cdx`.
- **State backup:** `~/.local/state/terminal-delight/premigration-sessions-20260910-124640`.
- **PR:** https://github.com/parker-brown-family/terminal-delight/pull/328
