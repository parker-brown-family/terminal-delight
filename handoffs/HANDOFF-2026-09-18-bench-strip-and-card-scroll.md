# Handoff — the agent strip's dials and verb, and the card that scrolls (2026-09-18)

## Status

Landed on `bench/kill-and-relaunch` in the worktree `~/Work/td-agent-kill`.
**5 commits ahead of `origin/main`, 1 behind** (`1c939d8`, a version/changelog
commit touching no `app/src` file — no collision). Working tree clean.

**NOT pushed. No PR opened.** Tie-off records; it does not ship.

**Installed** as `td-96c0535-kill-and-relaunch`, symlink swapped. The 35 running
terminal-delight processes keep their own inode — only a NEW window picks this
up.

Note: this branch predates main's `0.3.0` version bump, so the installed binary
reports `0.1.0`.

## What's done

| # | Change | Verified by |
|---|---|---|
| 1 | The AGENT strip's previously-empty `flex_1` slot carries model + effort dials and one verb — `END` while an agent is present, `LAUNCH AGENT` once it has gone | `strip_verb` + `dials_live` table tests, both mutation-checked |
| 2 | The card body scrolls: `ScrollHandle`, `overflow_y_scroll`, `Wheel::Card` | `wheel_target` test, mutation-checked; **not seen on a screen** |
| 3 | A waiting question draws below the body whatever else is on the bench (#533) | code read + the standalone parser harness; **not seen on a screen** |
| 4 | The launch offer is keyed on whether the pane HAS an agent, not on `bench.is_empty()` | `strip_verb(state, false)` table test |

`6e91c33` is the code (757 insertions, 4 files). `d18011f`, `8f2b885`, `f6f560a`
and `96c0535` are the two plan pages and their corrections.

## How to run / verify

```bash
cd /home/parker/Work/td-agent-kill/app && cargo fmt --all -- --check && cargo clippy --all-features -- -D warnings && cargo test
```

All three exit 0 as CI runs them; 1285 tests pass. Read the **exit codes**, not
grep hits.

```bash
cd /home/parker/Work/td-agent-kill/app && cargo build --release
```

The target dir is **`app/target/`**, not `target/` — there is no workspace root
`Cargo.toml`, and looking in the wrong one made a finished build look reaped.

To see any of it, open a NEW terminal-delight window and switch a pane to the
workbench face.

## Not done / next

- **Nothing here has been seen by an eye.** Three new controls, a new scroll
  container and a moved block — all four are *looks*, none photographed. The
  dial list is placed at `top 46px, right of the rail`, which is arithmetic
  against the strip's height rather than a measurement. Noted on **#505**.
- **#535** — `END` cannot end an agent that has stopped reading stdin. Rung two
  (signal the foreground pgid via a new host verb) is deliberately unbuilt.
- **#492** — nothing scriptable opens the dial list or presses the verb, so
  neither can be driven or photographed without a hand.
- **#533** stays open until this merges and installs.
- Codex's quit sequence is unverified; `END` sends claude's `0x03 0x03`.

## Watch out

- **`END` must never become a signal.** A signalled process never lowers the
  alternate screen, `host::next_mode` holds a pane at `Claude` while it is up,
  so a `SIGKILL` would permanently withhold the `LAUNCH AGENT` verb the button
  exists to restore. That is the whole reason for rung one.
- **`justify_end` + scroll = an unreachable top.** `body_anchor` now asks
  whether a card is *in* the body rather than whether one was *opened*; do not
  revert that, the stand-in card depends on it.
- **`ctx_read`/`ctx_edit` refuse this worktree** (outside the lean-ctx root
  jail). Use native `Read` → `Edit`; `ctx_shell` still works for commands.
  That cost 10 paged reads of `pane/bench.rs` this session.
- **Do not reinstall the same sha** — it deletes a live window's inode.
- Other agents are active in sibling worktrees (`td-attention`, `td-esc-floor`
  and `td-outer-paint` all moved during this session).

## Where it's recorded

- APES episode: `apes/projects/terminal-delight/episodes/2026-09-18-the-offer-was-asking-the-wrong-question.md`
- APES kanban: one ticket closed with deliverable, one opened (rung two), mirrored to #535
- lean-ctx: `ctx_session decision` breadcrumb (`ctx_knowledge` was NOT bound this session)
- file-memory: 3 new, 2 updated — see that dir's `MEMORY.md`
- Session harvest: `handoffs/2026-09-18-bench-strip-and-card-scroll.cdx` (165K, 7 facts, 361 secrets redacted)
