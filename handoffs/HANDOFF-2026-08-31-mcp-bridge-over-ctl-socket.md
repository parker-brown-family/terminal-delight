# Handoff — ctl→MCP bridge (2026-08-31)

## Status
**PR #212** (`feat/ctl-mcp-bridge` @ `28f8f43`) — pushed, **all checks green**,
`MERGEABLE / BLOCKED`, **not merged**. `main` is still `b99a014`.

⚠️ **The installed binary is ahead of `main`.**
`~/.local/bin/terminal-delight` **is** this PR's build (previous kept at
`terminal-delight.bak-pre-mcp-bridge`), and `~/.claude.json` registers the
`terminal-delight` MCP server against it. If #212 is closed rather than merged,
roll the binary back or the registration points at code that exists nowhere.

## What's done
- **`mcp_transport`** — ticker extracted as `start_bridge()` (the shared
  main-thread round-trip); `respond()` turns one JSON-RPC line into one response
  line. `serve_stdio` now calls it per stdin line, so socket and stdio run the
  same protocol code. *Verified:* 367 tests, clippy `--all-targets` clean.
- **`ctl`** — `mcp rpc <json>` (payload verbatim, served on its own thread so the
  accept loop never stalls) plus `mcp status|on|off`, `mcp writes on|off`,
  `mcp expose agents|all`. *Verified:* live round-trip against a scratch window —
  `initialize`, `tools/list`, `list_panes`, `get_pane_config`, `set_pane_config`.
- **`terminal-delight mcp`** — stdio relay; resolves its host window by walking
  `/proc` parents for a `ctl-<pid>.sock`. *Verified:* drove a live window end to
  end; refuses to guess when several windows and no parent match.
- **Default-deny holds** (the invariant that matters, since the bridge now starts
  unconditionally): policy off → reads *and* writes refused; reads on / writes off
  → only writes refused; both on → applies and repaints live.
- **The original task, applied live:** 9 panes + `outer` at stored `scale = 0.8`
  (gauge 80%) and `text_size = 0.75` (gauge 75%), persisted in `sessions/1.toml`,
  PATCH-clean (other channels untouched).

## How to run / verify
```bash
# build + the exact CI check set (fmt is NOT implied by clippy — see "Watch out")
cd ~/Work/terminal-delight/app
cargo fmt -- --check && cargo clippy --release --all-targets && cargo test --release

# grant the policy on a running window (off by default)
terminal-delight ctl mcp status --pid <PID>
terminal-delight ctl mcp on --pid <PID>
terminal-delight ctl mcp writes on --pid <PID>

# smoke-test the relay
echo '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"list_panes","arguments":{}}}' \
  | terminal-delight mcp --pid <PID>
```

## Not done / next
1. **Merge #212.** `BLOCKED/REVIEW_REQUIRED` is TD's **normal** state — all 180+
   prior PRs merged that way via admin bypass. `gh pr merge --admin` is
   classifier-blocked in auto mode; **leave auto mode (`shift+tab`)** and it
   prompts instead. Five other PRs are open from concurrent agents (#207, #209,
   #210, #213, #214) — none mine.
2. **#211** — OSD/API percent mismatch for `Scale`/`TextSize`. Not fixed here on
   purpose: behavioural change to a shipped API.
3. **#215** — relay parent-walk dies under tmux. Fix by stamping `TD_WINDOW_PID`
   into each pane's env.
4. **#216** — `main`'s review gate is unsatisfiable and requires **no** status
   checks; swap it for merge-on-green.
5. **`~/FOLLOWUPS.md` is broken** — reads 0 with 16+ labelled issues.
   `~/.config/gh-accounts/brown` was never created because `gh-accounts-reseed`
   does `intellimass` first, which has no token, and `set -e` aborts. Needs
   `gh auth login` for `parker-intellimass` (or a guard so one dead account can't
   abort the other). **An agent cannot fix this** — token handling is blocked.

## Watch out
- **`cargo fmt --check` is not implied by clippy + tests.** That exact gap turned
  #212's first run red; run the full triple above before pushing.
- **The live window's policy is left permissive:** window `834753` has
  `enabled=on writes=on expose=all`. `expose=all` reaches plain shell panes' cwd
  and scrollback. `terminal-delight ctl mcp expose agents --pid 834753` restores
  the safe default without turning the bridge off.
- **Shared tree is dirty with someone else's work** — `app/src/main.rs`,
  `app/src/pane.rs`, `docs/*.html` in `~/Work/terminal-delight` are a concurrent
  session's WIP. Untouched here. Build in a worktree.
- **Two other worktrees are live agents'** (`Work/td-logo-pr`, and three under
  `/tmp/.../1716e46e-*/scratchpad/`). Do not remove them.
- The terminal-delight MCP server showed `CONNECT_TIMEOUT` late in the session —
  expected when no window with a socket is reachable, or during a TD restart.

## Where it's recorded
- APES episode: `apes/projects/terminal-delight/episodes/2026-08-31-mcp-bridge-over-the-ctl-socket.md`
- Harvest: `955aa2c9-722e-487d-88d6-73dd7d8200a3.cdx` (130.3K, 269 secrets redacted)
- APES tasks: `…percent-units-mthll919`, `…survives-tmux-mthllcqe`, `…merge-on-green-mthllgm6`
- Memory: `td-agent-control-is-the-ctl-socket.md`, `followups-rollup-is-silently-empty.md`,
  `td-prs-need-parker-to-merge.md` (updated)
- PR #212 · Issues #211, #215, #216
