# Architecture: idle agents stop costing memory

Written as the record (Parker: "do all the work"); not stopped at for approval. Branch `feat/idle-agents` off `origin/dev/0.5.0` (1859de00), worktree `~/Work/td-authority-lab/idle-agents`.

## Fit

**Everything is in the window.** The session host (pid 2108452, `td-babe2dc-translations`) can never be restarted, so it can never learn a new verb; the host protocol stays at version 1 and this feature uses only what it already has (`ListPanes`, `SpawnPane`, `ClosePane`, `Push::Mode`). Every decision and every action is taken by the window process, which is swapped on every install.

**State that must outlive a window lives outside it:**

| State | Where it lives | Why there |
|---|---|---|
| A pane's memory group | a systemd transient scope `td-pane-<session>-<pane>.scope` under `app.slice` | made by the user manager, survives the window, found again by name |
| Resting | the kernel: `cgroup.freeze` = 1 on that scope | a new window reads it; nothing to persist |
| Asleep | the saved layout: the leaf carries `asleep` (when, and the wake line) and has no live pane | restore must not wake a sleeper; only a key does |
| Thresholds, switches | `~/.config/terminal-delight/agents.toml` (new), read by CONFIG › Agents | beside `launch.toml`, same `Option` discipline |

New modules:

- `panegroup.rs` — the OS plumbing: make/find a pane's scope over D-Bus (`busctl`), read `memory.current`, `memory.swap.current`, `cpu.stat`, `cgroup.procs`, freeze/thaw, `memory.reclaim`. No policy.
- `agentrest.rs` — the policy with no I/O: thresholds, the look at a pane, refusals, the wake launch line, the snapping slider's scale.
- `agentcfg.rs` (or an extension of the CONFIG page module) — the CONFIG › Agents page and `agents.toml`.

Touched:

- The window's per-pane periodic scrape (already 120 ms visible / 1 s hidden) gains a slow tick (every 15 s) that adopts a pane into its group once, reads its usage, and runs the policy.
- The window's single input choke point thaws a resting pane before any byte goes to the host; focusing a pane thaws it.
- The pane header draws the memory figure and the RESTING / ASLEEP chip.
- The agent counter at the top right gains two states.
- The saved leaf gains an optional `asleep` record; absent reads as awake (the old world).

## Endpoints

None. No host verb, no MCP verb in this pass. MCP's `list_panes` mode/state strings carry the new states where they already carry agent state.

## Data

- `agents.toml`: `rest = Option<bool>` (on/off), `rest_after_min = Option<u32>`, `sleep = Option<bool>`, `sleep_after_rest_min = Option<u32>`, `keep_awake = [pane keys]`; unset resolves to the shipped default at the point of use (10 min, 2 h), never written in.
- Saved leaf: `asleep = Option<Asleep { since: unix_ms, session: String, wake_file: PathBuf, model/mode words for the card }>`.
- `~/.local/state/terminal-delight/wake/<session>.sh`: the exact launch line, shell-quoted, `exec`'d by `bash <file>`. Mode 0600.

## Flow

**Adopt (every agent pane, once):** tick → pane has a shell pid and is hosted → `panegroup::adopt(session, pane_id, shell_pid)` → scope made with the whole tree named, stragglers attached → memory figure starts reading.

**Rest:** tick → `Look` assembled (focus, keep-awake, last touch, last output, Claude's own `~/.claude/sessions/<pid>.json` status + since, Codex screen state, foreign processes in the group, group CPU over the window) → `agentrest::may_rest` → freeze, wait for `frozen 1`, then `memory.reclaim` in steps → chip RESTING, counter +1. Any refusal is counted by reason.

**Wake from rest:** focus, key, paste, click or resize on the pane → thaw first (≤ 50 ms wait), then forward. Also: a window starting up thaws any frozen `td-pane-*` scope whose pane it does not know (an orphan must never hold a SIGHUP it cannot receive).

**Sleep:** tick on a resting pane → `may_sleep` (rested ≥ threshold, every rest refusal still clear, Claude only, conversation id from its own report, launch line rebuildable) → write the wake file → thaw → SIGTERM the agent pid only → wait ≤ 5 s for it and its self-report to go → confirm no process in the group still holds the conversation → `ClosePane` → leaf marked asleep, drawn as the sleep card over the last frame.

**Wake from sleep:** a key or a click on the card → `SpawnPane` for that leaf with recipe `bash <wake file>` (the sanctioned spawn-time path that session restore uses) → `asleep` cleared. A conversation is never reopened while any process still holds it.

## External

- systemd user manager over D-Bus (`busctl --user`): `StartTransientUnit`, `AttachProcessesToUnit`.
- cgroup v2 files under `/sys/fs/cgroup/user.slice/user-1000.slice/user@1000.service/app.slice/`.
- Claude Code's `~/.claude/sessions/<pid>.json` (read only).
- No environment variables required. A kill switch for the whole feature: `TD_AGENT_REST=0`.

## Found while building (2026-10-26)

- **Charges stay where pages were first touched.** cgroup v2 does not move a process's memory charge when the process moves, and the host starts every pane before the window can move it. A live rig showed 7 MB charged to a pane's group against a 264 MB Claude in it. So the pane's memory is read per process (PSS + SwapPss from `smaps_rollup`), and `memory.reclaim` on the pane's group reaches only what was touched since the move. For an adopted agent, rest's main effect is the freeze: it never re-touches its pages, so the kernel evicts them under pressure, and each page that cycles through swap comes back charged to the pane's group.
- **A woken agent is born in its group.** The window adopts a new pane as soon as it attaches, and the wake script waits up to 3 s until its own cgroup is the pane's before it `exec`s the agent. Measured on the second rig: 295 MB charged against 266 MB PSS, and its next rest pushed 176 MB out.
- **A hidden pane is not a focused one.** A pane in a tab that is not shown stops drawing, so its last-drawn focus goes stale; `focused_last_frame` requires the pane to be shown.
- **The wake key goes through `send`.** `on_key` may decide nothing outside `keylayer::route` (a guard test), so an asleep pane turns a key into a wake where keys become bytes, and chords the table handles still work. The socket gate drops every other write to an asleep pane.
