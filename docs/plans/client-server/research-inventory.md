> Recon output, 2026-09-08 — six parallel readers over the live tree plus one
> synthesizer, run before Gate 2. Inventory and seams only; no target design.
> Per-subsystem detail with file:line refs: research-maps.md

# terminal-delight — Client/Server Split Recon Inventory (Gate 2 input)

Merged from 6 parallel subsystem maps (process-model, ipc-surface, session-core, gui-shell, persistence, prior-art). All paths are relative to the live tree `/home/parker/Work/terminal-delight` unless noted. This is inventory and seams only — no target architecture; that happens at Gate 2 with Parker.

---

## 1. Current process model

One Rust binary, crate `terminal-delight` (`app/Cargo.toml`). gpui/gpui_platform/gpui_wgpu come by path from a pinned sibling `../zed-upstream` checkout (`zed_rev` in package metadata, applied by `scripts/prepare-gpui.sh` with ~6 out-of-repo patches: td-crt-pass, focus-blur, sever-gpl-crates, text-crawl, warp-tube-cap-32, glyph-transform). `alacritty_terminal` 0.26 is **stock crates.io, not forked** — the fork owns rendering effects only; all emulation is upstream.

`fn main()` (`app/src/main.rs:18333`) dispatches on argv[1], in order:

1. Six headless verbs that never touch gpui and exit as plain processes: `--td-emit-demo` (demo.rs:27), `ctl` (ctl.rs:873 — CLI client of per-window sockets), `mcp` (ctl.rs:1095 — stdio JSON-RPC relay), `agent-usage` (usage.rs:428), `agent-vitals` (vitals.rs:1729), `probe` (main.rs:18260 → session::probe_external).
2. Flag gate `flag_reply` (main.rs:18320): answers `--version`/`--help`, refuses other `-`-leading flags (exit 2).
3. **Anything else — including a typo'd subcommand — falls through to full GUI boot** and mutates on-disk session state. Test-pinned as intended at main.rs:17172 (reserved for a future "open in this directory" positional). This is issue #314's phantom-window behavior; changing it is a decision, not a bugfix.

GUI boot: `tty::setup_env()` → scratch-vs-adopt decision (`TD_SCRATCH`/`TD_SEED_CWD`/`TD_SEED_RESUME`/`TD_DEMO_STATE` force a non-persisting scratch window; otherwise `instance::resolve_session` (instance.rs:299) claims a session via kernel flock — `$TD_SESSION` > most-recently-saved unclaimed > fresh id, fail-closed to scratch) → `application().run` → **one gpui window per process**. Root entity `Workspace` (main.rs:1975) → `tabs: Vec<Tab>` (main.rs:627) → `Tab.root: Node = Tree<Entity<TerminalView>>`, a binary split tree, MAX_PANES=8 (main.rs:106, :75). Each `TerminalView` (pane.rs:1786) owns a `term::Session` (term.rs:60): `Arc<FairMutex<Term<EventProxy>>>`, alacritty `Notifier` (bytes in), unbounded event channel (events out), PTY master fd, shell_pid, generation counter. `term::spawn_in` (term.rs:105) starts alacritty's `EventLoop` thread (PTY reader + VTE parser). Dropping the pane drops the Session; that drop **is** the close (main.rs:77-82).

The event loop is gpui frames plus detached timer tasks: per-pane event pump + 800ms /proc foreground-mode watcher + 120ms effects/thinking clock (pane.rs:2455+); per-workspace 60ms jiggle, 2s dir-logo and transcript sweeps, 10s vitals sweep, optional keepalive writer, 30s state checkpoint, 150ms ctl-socket ticker, 40ms MCP bridge ticker (main.rs:2481+).

Every GUI window is also a server: a per-pid unix ctl socket (ctl.rs:491) and an MCP surface (mcp.rs policy, mcp_transport.rs transports). Tear-off = re-exec the binary with `TD_SEED_*` env (`spawn_seeded_window`, main.rs:18236). **There is no daemon**: N windows = N processes = N servers; discovery is a socket-dir scan plus Hyprland IPC; sockets are never unlinked on exit (swept by clients on failed connect).

> **Conflict:** the gui-shell map describes `resolve_session` as resolving a "per-Hyprland-workspace session key"; the session-core and persistence maps state the Hyprland workspace is a **ranking hint only** in adoption, not part of the key. (Per-workspace keys existed in the #194/#191 era per the prior-art map.) Both claims kept; Gate 2 should read instance.rs:299 fresh.

## 2. Existing IPC seams

A split should reuse these rather than reinvent.

| # | Seam | Contract | Files |
|---|---|---|---|
| 1 | **ctl unix socket** (per window) | `$XDG_RUNTIME_DIR/terminal-delight/ctl-<pid>.sock` (fallback `/tmp/terminal-delight-<uid>/`, 0700). One text line in, one line out: `ping`, `paint …`, `mcp status/on/off/writes/expose`, `adopt {json}` (opens a pane, optionally runs a resume line — spawns commands), `tabs [json ops]` (TabRef::Index or TabRef::Pane(shell pid)), `mcp rpc <json>`. Mutations queue over mpsc to a 150ms gpui ticker; status reads from atomic mirrors. Started unconditionally per window from `Workspace::build` (main.rs ~2923-2950). Never unlinked; stale-swept both sides. | app/src/ctl.rs (socket_path :321, start :491, run_cli :873); docs/features/12-ctl-scripting.md |
| 2 | **ctl CLI client + scope resolution** | Same binary: `--pid` / `--all` (socket-dir readdir) / `--workspace` (Hyprland IPC) / Owning (walk own /proc parent chain to the ancestor owning a socket; also identifies the caller's own pane). Fan-out with per-target tab-separated replies, exit 0/2. | app/src/ctl.rs (run_cli :873, owning_td, discover) |
| 3 | **MCP protocol core (pure)** | JSON-RPC 2.0, MCP rev 2025-06-18. Tools: list_panes, pane_events (transcript tail), get/set_pane_config (GradeReport 12 channels, ConfigPatch PATCH-semantics), leave_note, grep (≤50000 lines/pane, 50 matches/pane). Policy McpConfig{enabled:false, expose:AgentsOnly, writable:false} persisted per-window in the session toml; `TD_MCP_WRITE` forces writable. `mcp::handle_line_with` is a pure function of (line, Snapshot, closures). | app/src/mcp.rs; app/src/mcp_tail.rs; docs/features/01-agentic-mcp.md |
| 4 | **MCP transports + main-thread bridge** | One bridge (mpsc + 40ms gpui ticker, `UiReq::Snapshot/Apply/Search`, 5s snapshot budget). Stdio NDJSON server only when launched with `TD_MCP` — the only transport that gets the push feed (agent_appeared/vanished/tool_call, ~1Hz Watcher diff). `respond()` is a single line-in/line-out entry shared by stdio and the ctl socket's `mcp rpc` verb; a third transport needs only to call it. | app/src/mcp_transport.rs (start :99, start_bridge :155, respond :259) |
| 5 | **`terminal-delight mcp` stdio relay** | What agents actually register: forwards each stdin line as `mcp rpc <line>` to a running window's socket, target resolved by `--pid` → /proc parent-walk → single discovered socket (refuses to guess among several). 10s budget. **Gets no push feed** (bridge started with push=None). Parent-walk dies under tmux (#215). | app/src/ctl.rs (run_mcp_cli :1095); handoffs/HANDOFF-2026-08-31-mcp-bridge-over-ctl-socket.md |
| 6 | **Headless verbs as file/JSON contracts** | `probe <pid>` → JSON forensics from /proc; `agent-usage` → JSON records in `$XDG_STATE_HOME/{terminal-delight,omarchy}/agents/usage/`, newest-updatedAt-wins — already designed for a foreign writer (Omarchy); `agent-vitals` → transcript JSONL → bars JSON, differential oracle vs `scripts/td-agent-vitals.mjs`. | app/src/usage.rs (:280, :428), app/src/vitals.rs (:1729), app/src/session.rs (probe_external) |
| 7 | **Session files + flock arbitration** | `~/.config/terminal-delight/sessions/<id>.toml` + `<id>.lock` (kernel flock, dies with fd, SIGKILL-safe; release at quit-start with persistence disarmed first). The existing cross-process ownership seam. | app/src/instance.rs (claim_in :471, resolve_session :299, release :98) |
| 8 | **Hot-reloaded config files** | theme.toml (~300ms mtime poll), dir-logos.toml (2s sweep), keepalive away-flag file (presence-is-the-protocol, shared with herdr plugin). | app/src/theme.rs, app/src/dirlogo.rs, app/src/keepalive.rs (:142-172) |
| 9 | **Agent transcript stores (read-only)** | `~/.claude/history.jsonl`, `~/.claude/projects/<slug>/*.jsonl`, `~/.codex/sessions/**/rollout-*.jsonl`, plus the push ledger `~/.local/state/terminal-delight/agent-ledger/<pid>.json` (dir absent on this box — forensic fallback chain is the operating mode). Feed pane_events, resume synthesis, recover.rs tombstones. | app/src/session.rs (:124-232), app/src/mcp_tail.rs, app/src/recover.rs |
| 10 | **Hyprland IPC** | `$XDG_RUNTIME_DIR/hypr/<sig>/.socket.sock`: j/activeworkspace, j/clients (workspace scoping), focus dispatch. 250ms timeout. | app/src/instance.rs, app/src/ctl.rs, app/src/notify.rs |
| 11 | **TD as MCP host** | Launches plugin MCP servers over stdio NDJSON, 10s deadline, discovered from `~/.config/terminal-delight/plugins/*/plugin.json`. | app/src/plugins.rs |
| 12 | **Seeded-spawn env** | `TD_SCRATCH`/`TD_SEED_CWD`/`TD_SEED_RESUME` — the tear-off and td-send window-spawn API. `scripts/td-send` composes probe + `ctl ping --workspace active` + `ctl adopt` + seeded spawn. | app/src/main.rs (:18236), scripts/td-send |
| 13 | **Desktop notifications** | notify-send shell-out, action click → Hyprland focus + tab jump. | app/src/notify.rs |

## 3. Coupling hot-spots (ranked, hardest first)

1. **Shared-memory grid reads, every frame.** The UI locks the Term FairMutex synchronously and iterates cells in dozens of places: `styled_lines` (pane.rs:5027) per frame per pane, hit-testing, sync_size, grep_grid, recent_lines, the 120ms thinking-scan, FOCUS document builds (the one consumer that reads *entire* scrollback through the mutex). The read path assumes zero-cost shared memory, not message-passing. — app/src/pane.rs, app/src/term.rs.
2. **UI mutates emulation state in place.** Selection lives *inside* alacritty's Term (`term.lock().selection = …` in mouse handlers, send, extend_kbd_selection, pane.rs:4177-4241; autoscroll pane.rs:3391-3408); scroll position via `scroll_display` (wheel leg 3, read-nav). These writes have no seam at all today.
3. **Pid-based, machine-local identity everywhere.** Sockets keyed by pid in the filename; MCP/ctl write targets are shell pids; Owning/relay resolution walks `/proc/<pid>/stat` parent chains (fails under tmux, #215); pane_events keys on pid. The durable identity (resume-command session string) is explicitly *not* the addressing key. Session cross-wiring has bitten four times (#311, #299, #272, #151). None of it survives a process or network hop.
4. **Kernel-local introspection woven through the UI thread.** `session::capture` (tcgetpgrp on the PTY master + /proc cwd/cmdline) runs during every save, MCP snapshot, and tab-save; the 800ms mode watcher polls the master fd from a UI entity task; keepalive/vitals/usage read local /proc and local transcript files. Must run wherever the PTYs run (the std-only module layout supports it; nothing routes it). — app/src/session.rs:53-70, pane.rs:2416.
5. **All state access funnels through the gpui main thread with tight nested budgets.** relay 10s ≥ ctl mcp-rpc write 8s ≥ bridge snapshot/apply 5s, plus 150ms/40ms tickers. Any richer server API inherits this latency model; headroom under load (50k-line grep across panes in one 5s Search budget) is unmeasured. — app/src/mcp_transport.rs.
6. **Entity conflation.** TerminalView is a ~9k-line entity fusing emulator ownership, VT encoding, agent heuristics, theming/warp, and widget chrome; Workspace mixes the durable model (tabs/groups) into ~100 frame-transient fields; theme truth lives in gpui Globals (theme.rs). The class boundary is not the process boundary anywhere.
7. **Session identity claimed inside the GUI path.** The flock claim happens in `main()` before any window exists (instance.rs via main.rs:18422-18464); a server owning sessions must lift it out. The fail-closed arbitration itself is good prior art (see §5).
8. **One binary, one fragile dispatch ladder.** An unknown verb boots the GUI (#314); the stale-fork binary lacking headless verbs regressed every caller at once and caused the phantom-window incident (#312/#313). Client and server currently share a dispatch whose failure mode is mutating on-disk state.
9. **PTY writers besides the keyboard, all UI-side.** keepalive types into idle agent PTYs from a Workspace timer; adopt/resurrect/restore type resume lines at spawn; handle_term_event bounces PtyWrite back into the notifier (pane.rs:2963). All sit on the UI side of any would-be wire.
10. **Trust model is filesystem-only.** `parse_adopt` validation is "protocol hygiene, not a security boundary"; `adopt --run` spawns arbitrary commands. Any transport that leaves the 0700 runtime dir turns a paint-toggle surface into unauthenticated RCE. Multi-surface auth is recorded unsolved (#284).
11. **Renderer/global coupling.** warp.rs is a process-global `static Mutex<Vec<Tube>>` read by the patched gpui_wgpu CRT pass; cols/rows derive from font metrics measured via `window.text_system` — grid size is a client-side fact the server must be told.
12. **Push feed is stdio-only by construction** — the relay every in-pane agent uses gets no `notifications/message` (start_bridge push=None). A split that keeps the relay keeps the gap.

## 4. State a server must own (dies with the GUI process today)

- **The PTY masters and the entire child process tree** — shells, agents, and alacritty's EventLoop reader threads. GUI death kills every shell and agent; nothing streams PTY bytes anywhere else. Restore is respawn-plus-typing (`new_restored`, pane.rs:2455-2482), not process preservation.
- **Terminal grids and scrollback** — up to 10k lines/pane (alacritty `Config::default`) plus alt screen, behind the in-process FairMutex, never persisted anywhere. The single biggest loss on crash.
- **The flock session claims** (instance.rs) — kernel-local, same-filesystem semantics; whoever owns the PTYs must hold these.
- **The capture/persist loop** — 30s checkpoint, save chokepoint, and the /proc scrape that re-derives `claude --resume <id>` per checkpoint (session.rs:176-232). Exists *only* because process death severs the pane↔session binding; a server keeping PTYs alive obsoletes the whole forensic chain.
- **Live pane runtime state**: foreground-mode detection (800ms poll), sticky agent/alt-screen mode, bell "done" flags, keepalive stage machines, vitals bars, mcp_tail read offsets, in-composition (unposted) sticky notes, the `degraded`/`permit_shrink` save-guard flags, tool-prop/gamba/UI ephemera.
- **Live control-plane connections**: the ctl socket, the TD_MCP stdio server, plugin MCP child processes.

Already durable within 30s (a server does *not* need to own these to match today): layout tree, tab/group structure, window bounds, theme, per-leaf appearance/cwd/resume/name/logo/posted-note — all in the per-session TOML behind one chokepoint. Agent conversations survive independently in the vendors' own transcript stores (recover.rs can resurrect them). Explicitly lost today even with the TOML: shell history in-pane, non-agent foreground programs (vim, htop — ProbeKind::Other, classified non-faithful, session.rs:922-1005), and all scrollback.

## 5. Reusable assets

- **A transport-agnostic protocol core already exists.** `mcp::handle_line_with` is pure; `mcp_transport::respond()` is a single line-in/line-out entry already shared by two transports; framing is newline-delimited text everywhere. A third transport plugs in without redesign.
- **A truthful write round-trip exists.** `UiReq::Apply(Vec<ConfigUpdate>, Sender<Vec<ApplyOutcome>>)` (mcp_transport.rs:55) reports per-target outcomes — the model #308 says the ctl path should adopt.
- **The queue → main-thread-ticker pattern is proven** (ctl 150ms, bridge 40ms), with explicit budgets and back-pressure.
- **The emulation core is already server-shaped and gpui-free.** term.rs (bytes in / events out / resize / two kernel handles), session.rs, instance.rs, recover.rs, the generic `Tree<L>` split-tree ops, and the StateFile/SavedNode serde model are std+libc-only and unit-tested without gpui. The vitals engine is display-independent (#300 already asks for its extraction).
- **Persistence funnels through two tiny seams**: `instance::config_dir` (one path accessor) and `persist_primary_state` → `session::write_atomic` (atomic tmp+fsync+rename, shrink guard, `.last-good`, 10 rotated backups). Trivially relocatable behind a server.
- **Fail-closed ownership arbitration is solved.** claim_in refuses unarbitrated ownership; release() disarms persistence before freeing the lock. This is precisely the arbitration a server would centralize. Scratch/demo windows already model a zero-persistence client.
- **Restore-as-recipe.** The durable unit is (cwd, resume line, appearance) — a rebuild recipe, not live state — so a thin client's pane-rebuild story already exists. The generation counter (term.rs:69) is an existing cache-invalidation token consumers key on.
- **File contracts designed for foreign writers**: the usage-records seam (newest-updatedAt-wins, shared with Omarchy) splits for free; the keepalive away-flag and dir-logos/theme hot-reload are precedents for file-based coordination.
- **The substrate is verified free on this box**: `KillUserProcesses=no`, `Linger=yes` — a systemd --user daemon survives logout and starts at boot (docs/2026-08-29-foundation-interrogation-zed-gpui-quickshell.md).
- **The deliberation is pre-done.** Both prior "no" verdicts were conditional with recorded triggers; the composition is on record: the daemon slots *under* reincarnation (live PTYs answer crash/logout, reincarnation answers reboot), so #180's Reincarnation++ work is declared non-throwaway. Field survey exists (wezterm-mux, VS Code ptyHost/Agent Host, zmx, shpool, zellij). Contracts are documented in docs/features/12-ctl-scripting.md and 01-agentic-mcp.md; gate status in docs/plans/client-server/00-status.md (untracked).
- **td-send + seeded-spawn env** are a working out-of-process "create a window with content" API.

## 6. Consolidated open questions for Gate 2 (deduped across all six maps)

1. **What does "nothing lost" mean?** Is scrollback/output continuity across GUI death in scope (never persisted today; free if the server owns PTYs), and is keeping non-resumable foreground programs (vim, htop) alive an intended contract change to the migration story?
2. **Pane identity**: what durable, client-addressable pane id survives moves and session resume (today it's shell pid)? And is outside-addressability (e.g. a focus verb, #302) wanted at all — it crosses the deliberate appearance-only boundary.
3. **Session ground truth**: keep forensic inference (session.rs), adopt hook-pushed ids (#285, not approved), or both with inference as fallback? Design for ledger-present or ledger-absent? Related unresolved discrepancy: instance.rs claims "TD exports TD_SESSION into every pane" but no export site exists in app/src — stale comment or lost feature? (Conflict: kept both claims.)
4. **Session ownership in a client/server world**: who holds the flock; does adoption-by-recency (workspace as ranking hint — or per-workspace key, per the gui-shell map's conflicting reading) still make sense when sessions outlive windows; can two explicit `$TD_SESSION` launches contend?
5. **Write gating**: unified server policy — MCP's TD_MCP_WRITE gate with truthful per-target outcomes, ctl's ungated queue-ack, or something new? #308 says the asymmetry must be decided deliberately, not inherited.
6. **Trust and auth**: same-user 0700 unix trust is the entire security model, and `adopt` is arbitrary command execution. What is the auth story for any surface beyond it? Multi-surface authentication is recorded unsolved (#284).
7. **Grid read path**: server-side Term behind a generation/diff protocol, or client-side Term replica fed by the raw byte stream? (The frame-rate FairMutex reads and in-place selection/scroll mutations are the coupling this decision must cut; budget headroom is unmeasured.)
8. **Push feed**: the relay path gets no notifications by construction — deliberate contract or a gap the split must close?
9. **Binary shape and dispatch**: one binary or split client/server binaries; #314's unknown-verb semantics (refuse vs allowlist vs the reserved "open here" positional — the pinning test at main.rs:17172 must be rewritten either way); #300's gpui-free extraction; which checkout is canonical (#313 — and the client-server plan docs exist only untracked in ~/Work/terminal-delight).
10. **Discovery and addressing**: what replaces the pid-keyed socket-dir scan and /proc parent-walks (which fail under tmux, #215, and refuse to guess among windows)? Stale sockets are never unlinked and a recycled pid is an addressing hazard for any registry built on the dir listing.
11. **Product framing**: Gate 1 answers are pending from Parker (asked 2026-09-08), and no doc records whether the 2026-08-29 staged trigger ("build the daemon when mid-turn losses or logout frequency measurably hurt") has formally fired — the logout incident plus five external-client issues (#300, #302, #284, #308, #215) suggest it has.

**Pre-Gate-2 housekeeping flag** (not a design question): something — almost certainly the stale-fork binary from the phantom-window incident — is still writing the legacy `~/.config/terminal-delight/state.toml` and flat backups every ~30s. Identify and kill it before trusting any persistence measurements.

> **Verified dead, same day (2026-09-08):** `state.toml` and the newest flat
> backup were last touched 10:13:46 — the moment the phantom-window revert
> landed — and every running terminal-delight process (1 GUI + 28 `mcp`
> relays) executes the live-line build `td-7c28af7-ctl-tab-self`
> (`readlink /proc/<pid>/exe`, checked across `pgrep -f`). No live writer;
> what remains is residue from the stale-fork binary's morning (legacy
> `state.toml` + 30s-cadence flat `backups/`, cadence explains the recon
> reader's "every ~30s" impression). Cleanup of the residue belongs with the
> checkout collapse (#313).
