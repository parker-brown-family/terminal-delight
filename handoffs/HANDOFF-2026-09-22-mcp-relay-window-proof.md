# Handoff — the MCP relay proves its window (2026-09-22)

## Status

**Landed.** Three commits in `main`. One PR open and green.

| | |
|---|---|
| `26b20d4` | via PR 654 — the departure branch lets go of `wb_paused_ms`, plus a 15-field source gate |
| `d9e398f` | PR 664, closes #507 — `recorded_window` proves a window by asking it |
| `af0ae47` | PR 670, closes #391 — the relay rebinds instead of dying |
| PR **696** | OPEN, mergeable — `docs/decisions/0002`, closes #686. Branch `docs/the-ctl-socket-is-cooperative-trust`, worktree `~/Work/td-686`, pushed, clean |

## What's done

**The pause latch outlived its agent.** `wb_paused_ms` was cleared in the three
places a *turn* ends and not in the one place an *agent* ends, so a fresh agent
launched into a paused pane was born reading `Paused` with RESUME TURN offered
on a turn it never ran. Verified by mutation: deleting the clear fails
`the_departure_lets_go_of_every_field_that_names_the_conversation` by name.

**A socket file was taken as proof of life.** `recorded_window` ended in
`socket_path(pid).exists()`; nothing unlinks that file on exit (24 stale against
3 live windows, counted). Now one `proven_window` door asks `whoami` and checks
the session. The duplicate in `relay_target`'s `TD_SESSION` arm was not fixed
twice — it calls `recorded_window`.

**The relay died with its window.** `run_mcp_cli` froze its path before the loop
and returned 2 on a failed send, so a build cutover took every
`mcp__terminal-delight__*` tool out of an agent's session *permanently* — the
client does not un-fail a server when a healthy replacement spawns. Now it
rebinds once, via `rebound_window`, which **deliberately refuses**
`relay_target`'s "only one terminal is running, take it" arm.

## How to run / verify

```bash
cargo fmt --all --check
```
```bash
cargo clippy --locked --bin terminal-delight -- -D warnings
```
```bash
cargo test --bin terminal-delight -- --list | grep -c ": test"
```
```bash
cargo test --bin terminal-delight
```

**Reconcile `--list` against `passed + ignored` and refuse a count that does not
balance.** Last merged-tree run: 1581 listed = 1575 + 6, zero failed.

## Not done / next

- **#697 — the rebind arm has no test.** `rebound_window_at` is guarded; its
  caller is not. Needs a rig that spawns `terminal-delight mcp` against a socket
  it controls and kills the window underneath; none exists. APES:
  `test-the-relay-s-rebind-arm-not-just-its-resolver-mud0ue8q`.
- **PR 696 needs merging.** `rustfmt --check` was run; clippy and the suite were
  **not** run locally — comment plus markdown, CI covers it. Stated, not implied.
- `owning_td`'s `.exists()` at `ctl.rs:1393` stays, with the reason in the code.

## Watch out

- **The shared worktree is shared by ~5 agents.** `git status` there carries no
  authorship. Never `git add -A`; stage by path. Cut your own worktree **under
  `~/Work`** or the gpui path dep (`../../zed-upstream`) will not resolve.
- **A concurrent build in a shared target dir serves a stale binary.** It fired
  twice in one hour — once green-with-the-test-missing (1507 vs 1513), once
  green-then-`--list`-won't-compile. Hence the reconciliation rule above.
- **`paneident::tests::a_ledger_entry_binds_a_pane_in_a_crowd` and
  `a_real_child_named_like_an_agent_is_found_under_its_parent`** walk the live
  process tree and expect a different pid every run. They fail on a busy box and
  pass in CI. Not a regression.
- Two public retractions are on the record and were the right call: this
  session's outage was **not** proof of #507 (`locate()` asks live windows
  first), and "your uncommitted 413 lines" told a peer about a third agent's
  work.

## Where it's recorded

- APES episode — `projects/terminal-delight/episodes/2026-09-22-the-relay-that-believed-a-file.md`
- Harvest — `handoffs/2026-09-22-mcp-relay-window-proof.cdx`
- Decision — `docs/decisions/0002-an-agent-acting-for-you-is-you.md` (in PR 696)
- Memory — `the-mcp-relay-dies-with-its-window`, `git-status-in-a-shared-worktree-carries-no-authorship`
- Issues — #507, #391, #686 (closing), #697 (open)
