# Program Design: the War Room, first step

Status: **APPROVED 2026-10-22**: seven concurs, no notes, then "go!", on `reports/2026-10-22-war-room-program-design.html`. Signatures are written against `origin/dev/0.5.0` at `d17d0661`; existing names are quoted from that ref, and new ones are proposals. No implementation bodies.

## Files

New:
- `app/src/warroom.rs` — the rules, with no gpui: seats, lines, rungs, roles, `reach`, risk, bypass spending, and the in-flight ask book. Tested like `access.rs` and `hud.rs`.
- `app/src/askcopy.rs` — plans and runs one read-only copy: argv, cleaned environment, Claude's tool-list check, Codex version pin, and the answer.
- `app/src/askrecord.rs` — the append-only ask record (JSONL) and the two Gate 1 numbers read back from it.
- `app/src/war_room_view.rs` — draws the panel (seats, lines, seat facts, the switch row), beside `coordination_view.rs`, so `main.rs` does not grow by a panel.

Changed:
- `app/src/mcp.rs`:
  - `PaneInfo.exposed: bool` becomes `reach: warroom::Reach` plus `line: Option<warroom::LineView>`. Deleting the bool makes the compiler list every exposure site.
  - New verbs `ask_agent` and `ask_answer` go in `tool_defs`, `tools_call` and `provenance`, and `request_access` gains `ask_line`.
  - A new generic closure `ask: &A` joins `tail`, `apply`, `search` and `open`.
  - `list_panes` gains `hidden`.
- `app/src/mcp_transport.rs` — passes the `ask` closure into the window; it is the same bridge as `apply`.
- `app/src/access.rs`:
  - `Change` gains `AskLine` (on the wire as `ask_line`) and `Copy`. `Copy` is not on the wire: only the window raises it.
  - `Ask` gains `risk` and `ask_id`.
  - `nothing_to_ask` learns about lines.
- `app/src/attention.rs` — `AskRow` gains `risk`.
- `app/src/main.rs`:
  - `StateFile` gains `lines` and `ask_press_tokens`.
  - `SavedNode::Leaf` and its mirror `LeafFields` gain `seat` and `agents`.
  - `Workspace` gains `war_room`, `chart` and `asks`.
  - The counter's click and the `more-mcp` row open the War Room, and the help rows change.
  - The switch row is extracted as `mcp_switch_row`, shared by the wall and the War Room.
  - `rail_ink` takes a risk, and `rail_answer` handles `AskLine` and `Copy`.
  - The door test is rewritten.
- `app/src/pane.rs` — `TerminalView` gains `seat: Option<SeatId>` and `agents: AgentAccess`, which replaces `off_limits: bool`. `set_seat` is called only from the War Room.
- `app/src/vitals.rs` — reads a Codex rollout's last `token_count` and `turn_context` model into the same `Vitals.tokens` and `.model`.
- `app/src/workbench.rs` — `Bench::latest_response()`, the newest `Kind::Response` whatever the person has selected.
- `app/src/model_catalog.rs` — `Transport` becomes `pub(crate)` with `start_with(command, deadline)`. It is the shared runner, so no fourth spawner is written.
- `app/src/ctl.rs` — the refusal text names the War Room.
- `app/src/lang/*.rs` — every new string, in nine languages.

## Types & signatures

```rust
// ---- warroom.rs ----------------------------------------------------------

/// Minted once, when a person seats a pane; saved on the pane's leaf so it
/// survives any respawn. Pane ids restart with the host (host.rs:547).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SeatId(pub u64);
impl SeatId { pub fn mint() -> Self; }

/// A person's choice for one pane, for every agent (the read-only brief).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum AgentAccess { #[default] Open, ReadOnly, OffLimits }

/// How far a line reaches. First step: up to Ask. A word this build does not
/// know is kept as written and grants nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Rung { See, Read, Ask }
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RungWord { Known(Rung), Unknown(String) }

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Line {
    pub from: SeatId,
    pub to: SeatId,
    pub rung: RungWord,
    /// Some(n) = a bypass with n prompts left; None = a drawn line.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompts_left: Option<u8>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Chart { lines: Vec<Line> }
impl Chart {
    pub fn from_saved(lines: Vec<Line>) -> Self;
    pub fn saved(&self) -> &[Line];
    pub fn line(&self, from: SeatId, to: SeatId) -> Option<&Line>;
    /// Only the War Room panel and rail_answer call these (source test).
    pub fn draw(&mut self, from: SeatId, to: SeatId, rung: Rung);
    pub fn grant_bypass(&mut self, from: SeatId, to: SeatId, prompts: u8);
    pub fn erase(&mut self, from: SeatId, to: SeatId);
    pub fn forget_seat(&mut self, seat: SeatId);
    /// Spends one bypass prompt; the line disappears at zero.
    pub fn spend(&mut self, from: SeatId, to: SeatId) -> Result<Option<u8>, Refusal>;
    pub fn role_of(&self, seat: SeatId) -> Role;
    pub fn manager_of(&self, seat: SeatId) -> Option<SeatId>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role { Orchestrator, Manager, Worker, Alone }

/// What one agent gets of one pane, after every ceiling.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Reach { Hidden, Seen, Read, Ask }

pub struct Party { pub seat: Option<SeatId>, pub is_agent: bool, pub access: AgentAccess }

/// MCP off → Hidden. Off-limits → Hidden. A plain shell under agents-only →
/// Hidden. Unseated target → Read, never Ask (no prompt without a line).
/// Seated target → the line's rung, capped by the position (read-only caps
/// at Ask); no line → Hidden. `own` short-circuits for the agent's own verbs.
pub fn reach(cfg: &McpConfig, chart: &Chart, caller: &Party, target: &Party, own: bool) -> Reach;

/// TD's own write verbs on another pane: Read or better, an Open position,
/// writes on. Replaces `may_write`.
pub fn may_write(cfg: &McpConfig, reach: Reach, target: AgentAccess, own: bool) -> bool;

/// What the rail's colour reads. Table: figure 6 of the Gate 2 brief.
pub enum RiskFacts {
    Pane(crate::access::Change),
    Bypass { across_projects: bool, into_read_only: bool, prompts: u8 },
    Copy { reads: Option<u64>, limit: u64 },
}
pub fn risk(facts: &RiskFacts) -> crate::coordination::Severity;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
pub struct AskId(pub u64);

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Refusal {
    McpOff, NoLine, OffLimits, NotAnAgent, BypassSpent, OnePending,
    ConversationUnnamed, ConversationShared,
    ProgramUnproven { version: String }, ToolsWider { extra: Vec<String> },
}

#[derive(Clone, Debug, PartialEq)]
pub enum Stage {
    Priced { reads: Option<u64> }, AwaitingPress, Declined, Running,
    Answered { text: String, tokens_read: u64, cost_usd: Option<f64> },
    Refused(Refusal), Failed(String),
}

/// In flight, in memory only; question text never leaves this.
#[derive(Default)]
pub struct AskBook { /* HashMap<AskId, AskEntry> */ }
impl AskBook {
    pub fn open(&mut self, from: SeatId, to: SeatId, question: String) -> AskId;
    pub fn set(&mut self, id: AskId, stage: Stage);
    pub fn get(&self, id: AskId, asker: SeatId) -> Option<&Stage>;
}

/// The cheap answer, built by the window from facts it already has.
#[derive(Clone, Debug, Serialize)]
pub struct CheapAnswer {
    pub seat: SeatView,
    pub summary: Option<Summary>,   // Bench::latest_response()
    pub note: Option<String>,       // TerminalView::saved_note()
    pub recent_tools: Vec<crate::mcp::ToolEvent>, // the existing tail, 10
}
#[derive(Clone, Debug, Serialize)]
pub struct SeatView {
    pub title: String, pub project: Option<String>, pub role: Role,
    pub state: String,            // hud::AgentState, as worded on the counter
    pub model: Option<String>,    // None = unknown, never ""
    pub size: Option<u64>,        // None = unknown, never 0
}
#[derive(Clone, Debug, Serialize)]
pub struct Summary { pub brief: Option<String>, pub layman: String }
#[derive(Clone, Debug, Serialize)]
pub struct LineView { pub rung: Rung, pub bypass_left: Option<u8> }

// ---- askcopy.rs ----------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Program { Claude, Codex }

pub struct CopyPlan {
    pub program: Program,
    pub exe: PathBuf,          // /proc/<agent>/exe, as model_catalog does
    pub argv: Vec<OsString>,   // the measured switches, verbatim
    pub cwd: PathBuf,          // the worker's
    pub env: Vec<(OsString, OsString)>, // allowlist only
}
pub fn plan(program: Program, exe: &Path, conversation: &str, model: Option<&str>,
            cwd: &Path, agent_env: &[(OsString, OsString)]) -> CopyPlan;
/// HOME PATH USER LANG XDG_* CLAUDE_CONFIG_DIR CODEX_HOME; never TD_*.
pub fn keep_env(agent_env: &[(OsString, OsString)]) -> Vec<(OsString, OsString)>;
pub const CLAUDE_TOOLS: &[&str] = &["Read"];
/// Claude's system/init: tools ⊆ CLAUDE_TOOLS and mcp_servers empty.
pub fn check_init(init: &serde_json::Value) -> Result<(), Refusal>;
pub const CODEX_PROVEN: &[&str] = &["0.162.1"];
pub fn codex_proven(version_line: &str) -> Result<(), Refusal>;
pub const COPY_DEADLINE: Duration = Duration::from_secs(300);
/// Blocking: called on a worker thread, never the UI thread.
pub fn run(plan: CopyPlan, question: &str) -> Stage;

// ---- askrecord.rs --------------------------------------------------------

#[derive(Serialize)]
pub struct Entry<'a> {
    pub id: AskId, pub at_ms: u64, pub from: Option<SeatId>, pub to: Option<SeatId>,
    pub via: &'a str,           // "line" | "bypass" | "none"
    pub stage: &'a str,         // cheap|priced|pressed|declined|copy_started|answered|refused|failed
    pub question_len: usize, pub question_sha256: String,
    pub program: Option<&'a str>, pub model: Option<&'a str>,
    pub reads_tokens: Option<u64>, pub tokens_read: Option<u64>, pub cost_usd: Option<f64>,
}
pub fn path_for(session: &str) -> PathBuf; // $XDG_STATE_HOME/terminal-delight/warroom/<session>/asks.jsonl
pub fn append(path: &Path, entry: &Entry) -> io::Result<()>;
pub struct Numbers { pub unlined_answered: u64, pub tokens_per_answer_cheap: Option<f64>, pub tokens_per_answer_copy: Option<f64> }
pub fn numbers(path: &Path) -> io::Result<Numbers>;

// ---- mcp.rs (changed) ----------------------------------------------------

pub struct AskRequest { pub caller: Caller, pub pid: u32, pub question: String, pub copy: bool }
pub enum AskReply { Cheap(warroom::CheapAnswer), Started(AskId, Option<u64>), Awaiting(AskId, Option<u64>), Refused(warroom::Refusal) }
fn ask_agent<A>(args: &Value, snap: &Snapshot, ask: &A) -> Value where A: Fn(&AskRequest) -> AskReply;
fn ask_answer<A>(args: &Value, snap: &Snapshot, ask: &A) -> Value; // reads the book through the same bridge
// tools_call / dispatch / handle_line_full gain:  A: Fn(&AskRequest) -> AskReply

// ---- access.rs (changed) -------------------------------------------------

pub enum Change { OpenPane, ClosePane, WritesOn, ExposeShells, EventsOn, AskLine, Copy }
pub const ALL: [Change; 6];   // the wire changes; Copy is not among them
pub struct Ask { pub change: Change, pub pid: Option<u32>, pub title: Option<String>,
                 pub why: String, pub at: Instant,
                 pub risk: crate::coordination::Severity,
                 pub ask_id: Option<crate::warroom::AskId> }
pub struct Now<'a> { pub pane_off_limits: Option<bool>, pub mcp: &'a McpConfig,
                     pub line: Option<crate::warroom::Rung> }

// ---- main.rs (changed) ---------------------------------------------------

// StateFile:
#[serde(default, skip_serializing_if = "Vec::is_empty")] lines: Vec<warroom::Line>,
#[serde(default, skip_serializing_if = "Option::is_none")] ask_press_tokens: Option<u64>, // None = 400_000
// SavedNode::Leaf and LeafFields:
#[serde(default, skip_serializing_if = "Option::is_none")] seat: Option<warroom::SeatId>,
#[serde(default, skip_serializing_if = "Option::is_none")] agents: Option<String>, // "read-only"; unknown → OffLimits; off_limits = true wins
// Workspace:
war_room: bool, chart: warroom::Chart, asks: warroom::AskBook,
fn rail_ink(kind: attention::AttentionKind, risk: Option<coordination::Severity>, th: &theme::Theme, sk: &skin::Skin) -> Hsla;
fn mcp_switch_row(&self, th: &theme::Theme, s: f32, cx: &mut Context<Self>) -> gpui::Div;
fn ask_bridge(&mut self, req: &mcp::AskRequest, cx: &mut Context<Self>) -> mcp::AskReply;

// ---- elsewhere -----------------------------------------------------------

impl Bench { pub fn latest_response(&self) -> Option<&crate::surface::Response>; }
impl TerminalView { pub fn set_seat(&mut self, seat: Option<warroom::SeatId>, cx: &mut Context<Self>); }
// model_catalog.rs
pub(crate) struct Transport { /* unchanged fields */ }
impl Transport { pub(crate) fn start_with(command: &mut Command, deadline: Duration) -> Result<Self, String>; }
// vitals.rs: inside the per-mode read
fn codex_window(body: &str) -> Option<(u64 /*last input tokens*/, Option<String> /*model*/)>;
```

## Call stack

**The counter.** `render_agent_counter` click → `ws.war_room = true` → `war_room_view::render(ws, …)`. That draws `ws.mcp_switch_row(…)`, the seats (each from `agent_status()`, `agent_vitals[shell_pid]`, `bench.latest_response()`, `saved_note()`) and `chart.saved()`.

**A cheap ask.**
1. Agent → `mcp::handle_line_full` → `tools_call` → `ask_agent(args, snap, ask)`.
2. → `ask(&AskRequest{copy:false})` → transport bridge → `Workspace::ask_bridge`. It finds the caller by `Caller.pane`, set from peer credentials, and the target by pid.
3. → `warroom::reach(cfg, chart, caller, target, own)`, which must be at least `Ask`, else `Refused(NoLine | …)`.
4. → build `CheapAnswer` (seat facts, latest response, note, `tail(pane, 10)`).
5. → `askrecord::append(stage "cheap")` → `AskReply::Cheap`.
6. → `ask_agent` labels it untrusted terminal data, as `pane_events` is labelled.

**A copy.**
1. `ask_bridge(copy:true)` repeats the reach check, then `chart.spend` if the line is a bypass.
2. → `paneident::resolve` for the target. It must be `Bond::Declared`, and no other pane may hold the same `session_id`. Otherwise it refuses with `ConversationUnnamed` or `ConversationShared`.
3. → reads `reads = agent_vitals[target].tokens` (None = unknown) and `asks.open(…)`.
4. If `reads` is known and at most `ask_press_tokens`: set the stage to `Running` and `spawn_copy`. Otherwise:
   - build `access::Ask{change: Copy, risk: warroom::risk(Copy{reads, limit}), ask_id}`;
   - store it in the asker's `access_ask`, holding the `OnePending` rule;
   - set the stage to `AwaitingPress` and return `Awaiting(id, reads)`.
5. `spawn_copy`: on a background thread, `askcopy::plan(…)` → `askcopy::run(plan, question)`:
   - `Transport::start_with(cmd, COPY_DEADLINE)`;
   - for Claude, `send(initialize)` → `next()` reads `system/init` → `check_init`, which kills and returns `Refused(ToolsWider)` on any extra;
   - for Codex, `codex_proven(version)` first;
   - `send(user message)` → `next()`, repeated until `result`;
   - `Drop` kills the process group.
6. The result goes back to the window → `asks.set(id, stage)` → `askrecord::append` → a pushed `notifications/message`.
7. Agent → `ask_answer(id)` → bridge → `asks.get(id, asker)` → the stage, with the answer labelled untrusted.

**The press.**
1. `rail_answer(key, Yes)` → `access_ask.take()`.
2. For `Copy`: `asks.set(Running)` → `spawn_copy`.
3. For `AskLine`: `chart.grant_bypass(from, to, picked_count)` → `save`.
4. No → `Declined`, recorded.
5. `answer_line(...)` tells the asker, as today.

**Seating and lines (a person only).**
1. Click "Seat" on a pane in the War Room → `SeatId::mint()` → `TerminalView::set_seat` → `save`.
2. Click a seat, then another → `chart.draw(a, b, Ask)`.
3. Click the line to change its rung or erase it.
4. Taking a seat off → `chart.forget_seat`.

## Test plan

The rules, in `warroom.rs`:
- `mcp_off_hides_every_pane`
- `an_off_limits_pane_is_hidden_from_every_line` — a line of rung Ask into an off-limits pane gives Hidden.
- `a_seated_pane_without_a_line_is_hidden`
- `a_see_line_shows_a_seated_pane_and_nothing_more`
- `an_unseated_pane_is_read_and_never_asked` — no prompt without a line, even when Open.
- `a_read_only_pane_caps_a_line_at_ask`
- `an_unknown_rung_word_grants_nothing_and_survives_a_save` — round-trip keeps the word.
- `roles_come_from_lines` — drawn lines out and none in → orchestrator; lines in and out → manager; only lines in → worker; none → alone; bypass lines never count toward a role.
- `a_bypass_spends_one_prompt_and_vanishes_at_zero`
- `risk_matches_the_rail_table` — every row of the Gate 2 table.
- `an_unknown_copy_size_is_never_cheaper_than_warning`
- `may_write_refuses_a_read_only_pane_for_every_write_verb` — a matrix over verbs × positions × own.
- `a_seat_view_keeps_unknown_as_none` — no 0 size, no "" model.

The copy, in `askcopy.rs`:
- `the_claude_plan_carries_every_measured_switch` and `the_codex_plan_carries_every_measured_switch` — argv contains each flag from the evidence file.
- `the_copy_environment_never_carries_td_variables` — TD_TAG, TD_PANE_ID, TD_SESSION and any TD_* are absent.
- `the_copy_environment_keeps_only_the_allowlist`
- `an_init_with_a_write_tool_is_refused` / `an_init_with_any_mcp_server_is_refused` / `an_init_with_no_tool_list_is_refused`
- `an_unproven_codex_version_is_refused`
- `#[ignore] a_live_claude_copy_cannot_write` / `#[ignore] a_live_codex_copy_cannot_reach_the_web` — the probes as rig tests, run by hand on a version bump.

Which conversation:
- `a_guessed_conversation_is_refused_for_a_copy`
- `a_shared_declared_conversation_is_refused_for_a_copy`

The server, in `mcp.rs`:
- `list_panes_hides_seated_panes_without_a_line_and_counts_them`
- `ask_agent_without_a_line_is_refused_and_recorded`
- `ask_agent_cheap_answer_carries_summary_note_and_tools`
- `ask_agent_answers_are_labelled_untrusted`
- `ask_agent_copy_over_the_limit_waits_for_a_press`
- `ask_agent_works_with_writes_off` — Gate 2, question 7.
- `ask_answer_only_answers_the_asker` — another seat's id gives nothing.
- `every_tool_says_where_its_answer_came_from` (existing) covers the two new verbs.

The rail and asks, in `access.rs` and `main.rs`:
- `copy_is_not_a_wire_change` — an agent cannot raise the Copy card itself.
- `no_agent_facing_surface_can_answer_an_ask` (existing) is extended to AskLine and Copy.
- `only_the_war_room_and_the_rail_draw_lines` — a source scan of `Chart::draw`, `grant_bypass` and `erase`.
- `rail_ink_follows_risk_for_access_asks`
- `one_pending_ask_per_asking_pane`

The doors and saving, in `main.rs`:
- `the_counter_opens_the_war_room` — replaces `the_agent_wall_opens_from_the_counter_that_summarises_it`.
- `the_menu_row_opens_the_war_room`
- `only_ctrl_shift_a_opens_the_wall`
- `the_switch_row_is_drawn_by_one_function`
- `a_seat_survives_a_respawn` — `plan_attach` under a new pane id keeps the leaf's seat.
- `a_state_file_without_lines_reads_as_no_chart`
- `off_limits_wins_over_an_agents_word`

The record, in `askrecord.rs`:
- `the_record_never_holds_the_question_text`
- `every_stage_writes_one_line`
- `numbers_count_unlined_answers_as_zero_when_none`

Elsewhere:
- In `vitals.rs`: `a_codex_rollout_gives_its_last_call_size_and_model`.
- In `workbench.rs`: `latest_response_ignores_the_selected_card`.
- Each new gate gets a mutation that must fail a test, per the gates hub: `reach` returns Ask for an unseated pane; `keep_env` passes TD_TAG; `check_init` skips servers; the copy accepts `Bond::Sole`.

## Least confident decisions

1. **Promoting `model_catalog`'s private `Transport`** to the shared runner, with a deadline per call, instead of writing a fourth spawner. It touches a file another feature owns.
2. **One pending rail ask per asking pane**, reusing `access_ask: Option<Ask>`. An agent waiting on a bypass cannot also wait on an expensive copy.
3. **Seating and lines by click, then click** in the panel, not by dragging.
4. **A copy dies with the window.** Its process group is killed on drop, and the record shows `copy_started` with no answer. Five minutes is the deadline.
5. **The read-only position is saved as `agents = "read-only"`** beside the existing `off_limits`. `off_limits = true` wins, and an unknown word reads as off-limits.
6. **Answers from a copy are labelled untrusted terminal data** to the asker, like every other quote of another agent.
7. **A seat with several incoming lines is drawn under the oldest one,** and the rest are drawn as cross lines. Lines mean "may ask", so the chart is not forced to be a tree.
