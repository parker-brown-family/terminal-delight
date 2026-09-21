# Handoff — bench defaults, dial alignment, wall header (2026-09-19)

## Status

**Landed and installed.** `4e3a1d7` on `main` — PR #560 (four items) and PR #582
(the fold). Built, installed as `~/.local/lib/terminal-delight/td-4e3a1d7-fold`,
launcher repointed. Nothing uncommitted of mine.

**One step outstanding and it is Parker's:** his window still runs
`td-569f452-allin` (started 23:47) and needs a **window** restart — a new tile is
not a new process. The session host (`td-fc6957f-main serve --session 1`) holds
the PTYs, was not touched, and must not be restarted.

## What's done

| Change | Where | Verified by |
|---|---|---|
| New-agent defaults (harness / model / effort) in `~/.config/terminal-delight/launch.toml`, seeded into the LAUNCH AGENT panel | `app/src/launchpref.rs` (new), `Workspace::open_agent_launcher` | 6 unit tests incl. the absent-file branch and cross-harness clamping |
| `LAUNCH AGENT CONFIG` block at the top of the `Σ usage` card, folded by default | `Workspace::render_launch_defaults`, `launch_defaults_open` | photographed folded off the installed binary |
| One resolver for what a dial is on — press → `--model`/`--effort` off `/proc` → harness | `TerminalView::dial_now` in `app/src/pane/bench.rs` | the open list now lights the value the button shows |
| The open list drops under the dial that opened it, right-aligned | `workbench::dial_drop`, `benchdraw::probe`, `TerminalView::wb_bench_rect` | table test **watched to fail** against a regressed `dial_drop` |
| Return starts an agent on a bench with no agent and nothing selected | `workbench::return_launches` | table test incl. the card-is-open guard |
| Six duplicate state counters deleted from the agent-wall header (171 lines) | `app/src/main.rs` | photographed: header is name + `Δ`/`Σ` + two buttons |

Gates: `cargo fmt -- --check`, `cargo check --locked`,
`cargo clippy --locked -- -D warnings`, `cargo test --locked` — all green, 1310 +
30 tests. All five CI checks passed on both PRs.

## How to run / verify

```bash
cd /home/parker/Work/terminal-delight/app && cargo fmt -- --check && cargo clippy --locked -- -D warnings && cargo test --locked
```

Prove the installed binary is the new one (a version string proves nothing —
every build says `0.3.0`):

```bash
strings -a ~/.local/bin/terminal-delight | grep -c "LAUNCH AGENT CONFIG"
```

`1` on `td-4e3a1d7-fold`, `0` on everything before it.

Photograph a throwaway window without touching the live session — copy
`scripts/workbench-smoke.sh`'s shape, never try to place the window:

```bash
setsid env TD_SESSION="shot$$" TD_USAGE_DEMO=1 "$TD" > /tmp/w.log 2>&1 < /dev/null &
```

then find it by title `terminal-delight — $SESSION` in `hyprctl clients -j`,
`grim -g "<at> <size>"`, kill the pid, `pkill -f "serve --session $SESSION"`,
remove its `/run/user/1000/terminal-delight/session-$SESSION.*` files.

## Not done / next

- **terminal-delight#264** — the `</>` card clips its own content below ~870px.
  Commented with fresh evidence: the new block's title line is entirely outside
  the visible area in every capture this box can produce (throwaway windows land
  at 781px). The change was verified as *rendering* but never *read*.
- **context-delight#19** (filed) — `cdx-audit` is silent on a repeated GOAL. Four
  distinct commands failing at one impossible thing produced no finding, while
  three groups of genuinely-distinct commands sharing a `cd` prefix produced
  three. APES mirror on the `context-delight` board.
- **context-delight#15 / #17** — the prefix-matching `duplicate-action` bug,
  already open; acked again this session with an independent count (Bash 68
  calls / **68 distinct bodies**).

## Watch out

- **`~/Work/terminal-delight` is left on `bench/defaults-and-dials`**, fully
  merged. It cannot be switched to `main`: 24 untracked files there are a
  concurrent session's working set and two differ from what main now tracks, so
  a checkout would destroy their work. Use a throwaway worktree.
- **A merge can land while you are still pushing.** PR 560 merged at the commit
  before the fold. `git merge-base --is-ancestor <sha> origin/main` per commit;
  a green PR page is not the check.
- **A concurrent session committed its own feature onto my branch** between
  rounds, so 560 carried two stories. Do not rewrite a stranger's commit out of
  a shared branch — say so in the body instead.
- **A throwaway window with no `TD_SESSION` joins session 1** and spawns a real
  shell pane inside the live window. Always give it its own key.
- **The launcher symlink is contested** — it had already moved to
  `td-569f452-allin` twenty minutes before I touched it. If a restarted window
  does not show the change, read where `~/.local/bin/terminal-delight` points.
- Build a release for a cutover with `CARGO_TARGET_DIR` pointed at the primary
  worktree's `target/` — 34 seconds instead of a cold gpui build — and prove the
  artifact is main by comparing `git rev-parse HEAD^{tree}` with
  `origin/main^{tree}`.

## Where it's recorded

- **APES episode:** `projects/terminal-delight/episodes/2026-09-19-two-readers-of-one-fact.md`
- **APES kanban:** `give-a-new-agent-configurable-defaults-…-mu81s23s` (done, with
  deliverable); `give-cdx-audit-a-repeated-goal-category-…-mu81u0ab` on the
  `context-delight` board (backlog)
- **Harvest:** `handoffs/2026-09-19-bench-defaults-and-dials.cdx`
- **lean-ctx:** 4 knowledge facts (2 architecture, 2 deployment) + the session
  decision breadcrumb
- **File-memory:** `build-the-merge-before-it-merges` and
  `two-readers-of-one-fact-will-disagree` added;
  `dont-probe-hyprland-on-a-live-session`,
  `the-index-line-is-a-hook-not-the-fact`, `a-pr-can-merge-out-from-under-you`
  and `a-fresh-worktree-is-a-cold-gpui-build` updated
- **PRs:** https://github.com/parker-brown-family/terminal-delight/pull/560 ·
  https://github.com/parker-brown-family/terminal-delight/pull/582
