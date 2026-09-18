# Handoff — instance identity on the MCP surface (2026-09-18)

## Status

**Landed.** PR [#469](https://github.com/parker-brown-family/terminal-delight/pull/469)
merged 2026-09-17 as `de49d7b`. Five commits mine, plus `5a41e74` (rustfmt) and
`780cf08` (clippy) written by another hand to get CI green — see *Watch out*.

Live and confirmed in the installed build (`td-bd67a7a-title-vitals`): both
windows answer `whoami`, and a `list_panes` from this pane stamps
`session tdclip · window 1945289`.

## What's done

Several Terminal Delights run on one box; nothing on the wire said which one
answered, and an agent's relay picked its window out of `$TD_SESSION`.

| Change | Verified by |
|---|---|
| Every tool result carries `{session, window, build}` — text **and** `structuredContent.instance`, stamped in `tools_call`, errors included | unit tests confirmed to fail with the stamp removed; live stamp read from this pane |
| Panes carry `pane_id` (the host's `$TD_PANE_ID`), `null` when window-owned rather than omitted | unit test; live listing shows `pane 9` for this pane |
| Relay derives its session from the **kernel** — `session-<key>.sock` → listening inode in `/proc/net/unix` (state `01`) → `/proc/<pid>/fd` against its own parent chain | live, with the environment fully scrubbed, against a host that predates the change |
| A window refuses a call meant for another session, by name, before any UI hop | live: `wrong window: this is … "tdclip" … you asked for "1". Nothing was read or written.` |
| `leave_note` / `declare_deliverable` default to the calling pane | live: no-pid declaration landed on pid 714768, this pane |
| A relay meeting a pre-`mcp from` window steps back instead of hanging the agent | live, against the old tdclip window before the cutover |

979 tests at the time of merge.

## How to run/verify

```bash
cd /home/parker/Work/terminal-delight/app && cargo fmt --check && cargo clippy --locked -- -D warnings && cargo test --bin terminal-delight
```

Live smoke, from inside any pane (`jq` needed):

```bash
printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"list_panes","arguments":{}}}' | env -u TD_SESSION terminal-delight mcp | jq -r '.result.structuredContent.instance'
```

Expect the session you are actually in, with `TD_SESSION` scrubbed. Setting it
to another live session must **not** change the answer.

The wrong-window refusal, by hand:

```bash
terminal-delight ctl --pid <a window pid> mcp from not-its-session - rpc '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"list_panes","arguments":{}}}'
```

## Not done / next

- **[#507](https://github.com/parker-brown-family/terminal-delight/issues/507) — mine, and the first thing to pick up.** `recorded_window` proves a window with `socket_path(pid).exists()`, but ctl sockets are never unlinked, so a bounced window's corpse passes and the relay exits 2 with the agent's tools gone. Fix is `window_identity()`, which actually connects. APES ticket: `prove-a-recorded-window-by-talking-to-it-…-mu73hg5k`.
- **[#391](https://github.com/parker-brown-family/terminal-delight/issues/391)** — a window bounce kills every agent's long-lived relay and it does not return. Reproduced deliberately during this cutover; comment added.
- **[#215](https://github.com/parker-brown-family/terminal-delight/issues/215)** — tmux reparents its server away from the pane, so no ancestry walk reaches the host. `$TD_SESSION` is still the only route there, deliberately.
- **[#465](https://github.com/parker-brown-family/terminal-delight/pull/465)** — pane-mode reconciliation. **This, not instance identity, is why the attention rail reads 0.** Still unmerged, and it touches `host.rs`, so session 1 needs a host restart to get it.

## Watch out

- **Run CI's gates, not the suite.** `.github/workflows/ci.yml` runs `cargo clippy --locked -- -D warnings` and rustfmt. I ran the suite five times and neither gate; two commits of someone else's time paid for that.
- **Don't put required fields on the session host's hello.** It owns the PTYs, so the feature is unavailable until someone ends every pane — and fails against every host already running. Derive from the kernel instead. This is why the final design asks `/proc`, not the host.
- **`/proc/net/unix` lists peers beside the listener** — fourteen rows for one live session socket, thirteen of them connections. Match state `01` or you get an inode nobody holds.
- The relaunched window landed on the **active workspace, not 6**; this Hyprland's Lua build exposes no move dispatcher, so it gets dragged back by hand.
- The worktree and `~/.local/bin/terminal-delight` are contested by concurrent sessions — the launcher moved to another agent's build twice during this work.

## Where it's recorded

- APES episode: `apes/projects/terminal-delight/episodes/2026-09-18-two-terminal-delights-and-only-one-of-them-answered.md`
- APES kanban: identity ticket closed with deliverable; #507 mirror opened
- lean-ctx: `ctx_session` decision breadcrumb (design rule + next steps)
- file-memory: `a-required-field-belongs-where-you-can-restart`, `run-the-gates-ci-runs`
- Session harvest: `handoffs/2026-09-18-instance-identity.cdx`
- Brief: `reports/2026-09-16-two-instances-one-identity.html`
