# Handoff — </> LeanCTX token-savings plugin (2026-06-22)

## Status
**LANDED + DEPLOYED.** PR **#85** squash-merged → main `1de4f46`; pushed. Main has since advanced to `bb906af` (concurrent #89 master-lock + #91 rustfmt-of-this-plugin, both on top of #85). Binary redeployed at `1de4f46` (note: 2 commits behind current main — redeploy to pick up #89/#91). Parker's session running on the fresh build.

## What's done
- **TD plugin #2 `leanctx-savings`** — `</> savings` button on the AGENT WALL rollup → flat (warp-suppressed) overlay: global trio (tokens saved / compression % / USD) + Lv badge + per-agent (≈ estimated) + top-files. Verified: 184 tests (incl. #79's `house_terminal` + new `savings_view`), clippy clean, rendered headless on :1.
- `plugins/leanctx-mcp/{leanctx-mcp,plugin.json}` — self-contained Python MCP server (tool `savings`) wrapping `lean-ctx gain --json` + `cost_attribution.json`. Smoke-tested standalone (real: ~69M saved, 144 agents).
- Installed to **`~/.local/bin/leanctx-mcp`** so the *installed* TD binary's resolver finds it (the exe-ancestor path only works from the repo checkout).
- `docs/2026-06-21-leanctx-savings-plugin-gap-analysis.md` (on main).

## How to run/verify
- Use it: in TD press **Ctrl+Shift+A** (agent wall) → click **`</> savings`**.
- Rebuild/redeploy after pulling main: `/td-redeploy` (release build + hot-swap `~/.local/bin/terminal-delight-main.bin`), then **restart TD** (quit + relaunch — master-lock window).
- Demo/screenshot (leak-safe, fictional data): launch with `TD_SAVINGS_DEMO=1` on a staged `:1` instance (see `/td-demo`).
- Plugin self-test: `printf '...' | ~/.local/bin/leanctx-mcp` (init + tools/call savings) → JSON with `tokens_saved`.

## Not done / next
1. **lean-ctx per-agent attribution** (the core gap): savings ledger tags `agent_id:"local"`. Parker's next deliberate phase — upstream issue (`github.com/yvgude/lean-ctx`) with `TD_SAVINGS_DEMO` screenshots, then the change in a **parker-brown-family fork** so per-agent drops the `≈`.
2. **Redeploy** to pick up #89/#91 (deployed binary is `1de4f46`, main is `bb906af`).
3. **Plugin bundling gap** → escalated as a GitHub issue: `/td-redeploy` should install bundled plugins to `~/.local/bin`. (I hand-cp'd it this time.)

## Watch out
- **Shared worktree.** main moved `1de4f46`→`bb906af` mid-session via concurrent agents (#89/#91) + a `feat/master-lock` worktree at `~/td-master-lock`. Before committing dirty `main.rs`, superset-check vs the merged base (`git merge-file` silently dropped #79 lines here — see the episode).
- **TD restore = MASTER-window flock**, not comm. A 2nd instance is a scratch pane by design; restore needs the master to quit then relaunch (a detached pid-watcher automates it). Never `pkill` TD (Claude runs in a pane).
- `docs/handoffs/HANDOFF-2026-06-22-agent-wall-filters.md` is **another agent's** untracked handoff — not mine; left alone.

## Where it's recorded
APES episode: `apes/projects/terminal-delight/episodes/2026-06-22-leanctx-savings-plugin-and-a-shared-tree-rebase.md` · lean-ctx: keys `leanctx-savings-data-model` + 2 gotchas + session decision · file-memory: `leanctx-savings-plugin.md`, `shared-tree-rebase-superset-guard.md` · PR #85 · session harvest `.cdx` in this dir.
