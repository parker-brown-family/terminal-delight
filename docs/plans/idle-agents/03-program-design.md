# Program design: idle agents stop costing memory

Record, not a stop (authorization in `00-status.md`). Paths under `app/src/` on `feat/idle-agents`.

## Files

| File | Why |
|---|---|
| `panegroup.rs` (new) | OS plumbing: a pane's systemd scope, its memory and CPU, freeze/thaw, `memory.reclaim`, the input `Gate`, the orphan sweep. No policy. |
| `agentrest.rs` (new) | Policy with no I/O: `Look`, `Refusal`, `may_rest`, `may_sleep`, `foreign`, `wake_argv`, the slider `Scale`/`Detent`s, `PaneRest` (the per-pane state the view carries), `Dormancy`. |
| `agentrest_io.rs` (new) | The background half of one policy tick: adopt, measure, classify processes, act (freeze+reclaim, or thaw+SIGTERM). Runs on the background executor; returns an `Outcome`. |
| `agentcfg.rs` (new) | `~/.config/terminal-delight/agents.toml` (`Option` fields, defaults resolved at use) |
| `vt/socket.rs`, `term.rs` | `AttachStreams.gate` → `SocketSource` calls `gate.on_input()` before every write. Resize does not thaw. |
| `pane.rs` | `TerminalView.rest: PaneRest`; touch on key/paste/click/focus; last output from `content_generation`; `agent_status` folds dormancy after the bell; header chip and memory figure; an asleep pane's keys and clicks emit `WakeAsleep` instead of reaching the shell; the sleep card. |
| `hud.rs` | `AgentState::{Resting, Asleep}`; badge, label, label_in, glow; not `needs_you`, not `is_quiet` (so a latched bell cannot turn them into Finished). |
| `main.rs` | `rest_idle_agents` loop beside `reconcile_modes`; `apply_rest`; `wake_sleeper` (respawn into the leaf); counter cells; `slot_tally` counts an asleep pane although its mode is Shell; `SavedNode::Leaf.{asleep, keep_awake}`; restore keeps a sleeper asleep; dead-agent list treats a sleeping conversation as live; CONFIG menu entry 🤖 Agents and its overlay. |
| `slot.rs` | `Tally.{resting, asleep}`. |
| `lang/chrome.rs`, `lang/*` | New strings in all nine tables. |

## Types and signatures

```rust
// panegroup.rs
pub struct PaneGroup { pub unit: String, pub dir: PathBuf }
pub fn unit_name(session: &str, pane: u64, shell: i32) -> String; // td-pane-<key>-<pane>-<shell>.scope
pub fn adopt(session: &str, pane: u64, shell: i32) -> Result<PaneGroup, AdoptError>;
pub fn usage(g: &PaneGroup) -> Usage;                // Option<u64> fields
pub fn procs(g: &PaneGroup) -> Vec<i32>;
pub fn set_frozen(g: &PaneGroup, on: bool, within: Duration) -> bool;
pub fn reclaim_paced(g: &PaneGroup) -> Option<u64>;  // bytes freed, 64 MiB steps, stops when a step frees < 8 MiB
pub fn listening_inodes() -> HashSet<u64>;           // /proc/net/tcp{,6} state 0A
pub fn proc_info(pid: i32, listening: &HashSet<u64>) -> Option<agentrest::Proc>;
pub fn orphans(session: &str, live: &[(u64, i32)]) -> Vec<PaneGroup>; // td-pane-<key>-* not in `live`
pub struct Gate { held: Mutex<Option<PathBuf>>, writes: AtomicU64, woke: AtomicBool }
impl Gate {
    pub fn on_input(&self);                                   // pump thread: count, thaw if held
    pub fn writes(&self) -> u64;
    pub fn freeze_if_quiet(&self, g: &PaneGroup, epoch: u64) -> bool;
    pub fn release(&self) -> bool;                            // thaw without counting as a wake
    pub fn take_woke(&self) -> bool;
}

// agentrest.rs
pub enum Dormancy { Awake, Resting { since: Instant }, Asleep(Asleep) }
pub struct Asleep { pub since_ms: u64, pub session: String, pub wake_file: PathBuf, pub model: Option<String>, pub lost: Vec<String> }
pub struct PaneRest { pub gate: Arc<Gate>, pub group: Option<PaneGroup>, pub last_touch: Instant, pub last_output: Instant,
                      pub seen_generation: u64, pub dormancy: Dormancy, pub keep_awake: bool,
                      pub refusal: Option<Refusal>, pub usage: Usage, pub given_back: Option<u64>, pub cpu_prev: Option<(Instant, u64)> }

// agentrest_io.rs
pub struct Job { pane: u64, shell: i32, session: String, gate: Arc<Gate>, group: Option<PaneGroup>, look: Look,
                 dormancy: DormancyKind, rested_for: Option<Duration>, cpu_prev: Option<(Instant,u64)>,
                 thresholds: Thresholds, sleep_enabled: bool, epoch: u64, mode: Option<Agent> }
pub enum Act { Nothing, Rested { given_back: Option<u64> }, Woke, Slept(Asleep), }
pub struct Outcome { pane: u64, group: Result<PaneGroup, String>, usage: Usage, cpu: Option<(Instant,u64)>, act: Act, refusal: Option<Refusal> }
pub fn run(job: Job) -> Outcome;
```

## Call stack

- **Tick (15 s):** `rest_idle_agents` → main: `rest_jobs()` (one `Job` per hosted pane; facts from the view) → background: `agentrest_io::run` per job (adopt → usage → agent pid via `vitals::agent_under` → Claude self-report → processes → `may_rest`/`may_sleep` → act) → main: `apply_rest(outcomes)` (store, set dormancy, notify) → `panegroup::orphans` → thaw any frozen orphan.
- **Wake from rest:** any write to the pane → `SocketSource::write` → `gate.on_input()` → thaw under the gate's lock → bytes go on. The housekeeping tick sees `take_woke()` → `Dormancy::Awake`, touch. Focus edge in `render` → `gate.release()` + touch.
- **Sleep:** background `run` with a resting pane past the threshold → `may_sleep` → write wake file → `gate.release()` → `kill(agent, SIGTERM)` → wait ≤ 5 s for the pid and its self-report to go → `Act::Slept`. Never SIGKILL: an agent that does not stop is left awake with a refusal.
- **Wake from sleep:** asleep view `on_key`/click → `cx.emit(WakeAsleep)` → `wire_pane` subscription → `Workspace::wake_sleeper` → `make_pane_host_spawned({cwd, resume: "bash '<wake file>'"})` → `adopt_presentation` → `replace_leaf` → `ClosePane` on the old shell pane.
- **Restore:** `SavedNode::Leaf.asleep` → `plan_attach` binds the leaf to its live shell pane as before → the built view gets `Dormancy::Asleep`. If the shell pane is gone, the leaf is respawned as a bare shell (recipe dropped) and stays asleep.

## Test plan

- `agentrest`: the 14 tests already passing (rest/sleep refusals, unknowns, Codex, self-report, foreign classes, wake line, quoting, detents, scale).
- `panegroup`: the 7 passing, plus `a_unit_name_carries_the_shell_pid`, `an_orphan_is_any_scope_of_this_session_not_live`, `the_gate_thaws_only_what_it_froze`, `the_gate_refuses_to_freeze_after_a_write`, `listening_inodes_parse_only_listen_rows`.
- `hud`: `a_resting_or_sleeping_agent_is_never_promoted_by_the_bell`, `the_two_new_states_read_in_every_language`.
- `main`: `a_sleeping_leaf_round_trips_and_absent_reads_awake`, `a_sleeping_leaf_is_not_respawned_with_its_recipe`, `the_counter_has_a_cell_for_every_state`, `an_asleep_pane_counts_although_its_mode_is_shell`.
- Mutation checks: break the gate's epoch check, the bell order, the unknown refusals, the detent table; each must turn a test red.
- Live check before install: one disposable pane in a scratch session through adopt → rest → key → wake.

## Least confident decisions

1. CPU ceiling 3% of a core over the window: from ten samples of idle Claude (0.5–1.2%).
2. Resize does not thaw. A re-tiled resting pane shows its old frame reflowed until touched.
3. Rest touches no Codex helper (they live in Codex's service), so a resting Codex pane saves ~20 MB; it is still rested for the CPU.
4. systemd-oomd is active on `app.slice` with swap and pressure kills. Per-pane groups make a single pane a kill candidate instead of the whole Terminal Delight scope, and a resting pane holds most of its memory in swap. Swap was 64% used; oomd's swap kill starts at 90%.
5. The sleep shell stays (Path B): 4 MB per sleeping pane, in exchange for a stable pane id and graceful degradation on an older window.
