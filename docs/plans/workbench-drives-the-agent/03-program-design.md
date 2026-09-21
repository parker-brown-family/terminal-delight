# Program Design: The workbench drives the agent

Written from the code as built on 2026-09-21 in `~/Work/td-decouple`
(`plan/workbench-decouple-async`, from `origin/main` at `4b1fb2b`), not ahead of
it: Parker was AFK and asked for the implementation in the same sitting, so this
page is the reviewable record of the decisions that would otherwise have been
made silently mid-implementation — and were, and are written down here.

## Files

| File | Why it lives there |
|---|---|
| `app/src/channel.rs` — new | The channel's types, parsers, encoders and per-pane state. No gpui, no pane, no window, so it tests like `surface.rs`: against values. The one place a record becomes an effect. |
| `app/src/surfacefeed.rs` | The mailbox is this module's subject; the inbound journal is one more thing in it. `tail_inbound`, `write_marker`, `write_answers`, `outbound_path`. |
| `app/src/surface.rs` | `Origin::Hook`: provenance is a property of a surface, drawn by the card, and this is where every other origin is. |
| `app/src/workbench.rs` | `Line` becomes a document (breaks, rows, undo); `line_edit` takes shift; `Bench::with_question_mut`. The decisions stay in the module with the table tests. |
| `app/src/pane/bench.rs` | The wiring: what a key does on the workbench face, what SEND does, what a press on a hook-carried card does, and the two new pane verbs the workspace calls (`channel_events`, `bench_beacon`). Renderers unchanged. |
| `app/src/pane.rs` | Five new fields on `TerminalView`; `needs_input` ORed with the channel; the ask latch yields to the hook; the departed edge resets the channel; `present` counts responses. |
| `app/src/main.rs` | `mod channel`; the sweep hands events to panes and refreshes the marker. |
| `scripts/td-agent-hooks` — new | The Claude Code adapter. Bash and `jq`, like the ledger hook beside it, because it runs inside every hook invocation and must never break a session. |
| `scripts/install-agent-hooks.sh` — new | Same shape as `install-recovery-hook.sh`: `jq` merge into `~/.claude/settings.json`, backup kept, `--uninstall`. |
| `scripts/td-agent-hooks.test.mjs` — new | Drives the real script with real payloads in a scratch `XDG_STATE_HOME`. |
| `docs/spec/td-agent-channel.md` — new | The contract, versioned like TDSP. |

## Types & signatures

### `channel` — the record types

```rust
pub const TDAC_VERSION: &str = "0.1";
pub const MARKER_FRESH_MS: u64 = 4_000;
pub const ROUNDS_KEPT: usize = 16;
pub const HISTORY_KEPT: usize = 32;

/// One question of a round, verbatim from the harness's tool input.
pub struct Asked { pub question: String, pub header: Option<String>, pub multi: bool, pub options: Vec<AskedOption> }
pub struct AskedOption { pub label: String, pub description: Option<String>, pub preview: Option<String> }

/// One line of inbound.jsonl. `at_ms` is Option: a record that did not say when
/// is ordered by its place in the file, never by a zero.
pub enum Inbound {
    Prompt   { at_ms: Option<u64>, prompt_id: Option<String>, text: Option<String> },
    Question { at_ms: Option<u64>, tool_use_id: String, questions: Vec<Asked>, deadline_ms: Option<u64> },
    Waiting  { tool_use_id: String, until_ms: u64 },
    Released { tool_use_id: String, why: String },
    Answered { tool_use_id: String, answers: Option<Value> },
    Reply    { at_ms: Option<u64>, text: Option<String> },
    Notify   { at_ms: Option<u64>, kind: Option<String>, message: Option<String> },
    Unknown  { type_name: String },          // kept and counted, never dropped
}
impl Inbound { pub fn parse(v: &Value) -> Option<Inbound>; pub fn parse_line(line: &str) -> Option<Inbound>; }

pub enum Delivery { Paste, Flat, Held }
pub enum Route { File, Keys, Sentence }
pub enum Outbound {
    Say { id: String, text: String, delivery: Delivery },
    Answer { tool_use_id: String, answers: Map<String, Value>, route: Route },
    Interrupt,
    End,
    Keys { bytes: Vec<u8>, why: String },     // the one impure verb, always journaled
}
impl Outbound { pub fn to_json(&self, at_ms: u64) -> Value; }

/// ONE write: bracketed paste + CR, or flat + CR when the terminal cannot.
pub fn say_bytes(text: &str, bracketed: bool) -> (Vec<u8>, Delivery);
/// The first open road, in order: file, keys, sentence.
pub fn route_for(waiting_until_ms: Option<u64>, released: bool, now_ms: u64, cursor_known: bool) -> Route;
pub fn marker(bench_open: bool, at_ms: u64, window_pid: u32) -> Value;
pub fn marker_holds(v: &Value, now_ms: u64) -> bool;   // workbench face AND |now - at| < FRESH
pub fn answers_json(tool_use_id: &str, answers: &Map<String, Value>) -> Value;
pub fn file_key(tool_use_id: &str) -> String;          // the hook applies the same filter
pub fn reply_surface(text: &str, now_ms: u64) -> Option<Surface>;
```

### `channel` — the per-pane state

```rust
pub struct Round {
    pub tool_use_id: String,
    pub at_ms: Option<u64>,
    pub questions: Vec<Asked>,
    /// Per question: option indexes picked. None = unanswered; Some([]) = a
    /// multi-select nobody has ticked. Different facts.
    pub picked: Vec<Option<Vec<usize>>>,
    pub waiting_until_ms: Option<u64>,
    pub released: bool,
    pub closed: bool,   // the harness said the tool returned
    pub sent: bool,     // the bench has answered, whichever road
}
impl Round {
    pub fn surface_id(&self, i: usize) -> SurfaceId;            // ask-hook-<key>-<i>
    pub fn complete(&self) -> bool;
    pub fn answers(&self) -> Option<Map<String, Value>>;        // None until complete
    pub fn surfaces(&self, now_ms: u64) -> Vec<Surface>;        // one TDSP question per question, Origin::Hook
}

pub enum Effect { Asked { text: String }, Present(Vec<Surface>), Reply { text: String }, Nothing }
pub enum Press {
    WriteAnswers { tool_use_id: String, answers: Map<String, Value> },
    Keys { bytes: Vec<u8>, note: String },
    Sentence { label: String },
    Recorded,
    Refused(String),
}

pub struct State { /* rounds, cursors, unknown, heard, responses_since_prompt */ }
impl State {
    pub fn take(&mut self, ev: Inbound, now_ms: u64) -> Effect;
    pub fn saw_response(&mut self);
    pub fn has_open_question(&self) -> bool;
    pub fn owns(&self, id: &SurfaceId) -> Option<(usize, usize)>;
    pub fn matching(&self, question_text: &str) -> Option<SurfaceId>;   // the screen-merge
    pub fn saw_cursor(&mut self, id: &SurfaceId, cursor: usize, submit: Option<usize>);
    pub fn round_surfaces(&self, id: &SurfaceId, now_ms: u64) -> Vec<Surface>;
    pub fn press(&mut self, id: &SurfaceId, nav: usize, now_ms: u64) -> Press;
}
```

### `surfacefeed`

```rust
pub struct Feed { seen: HashMap<PathBuf, Stamp>, offsets: HashMap<PathBuf, u64> }
pub struct Arrivals { pub pane: u64, pub posts: Vec<Post>, pub events: Vec<channel::Inbound> }
impl Feed { pub fn tail_inbound(&mut self, dir: &Path) -> Vec<channel::Inbound>; }
pub fn outbound_path(session: &str, pane: u64) -> PathBuf;
pub fn write_marker(dir: &Path, bench_open: bool, now_ms: u64) -> io::Result<()>;
pub fn write_answers(dir: &Path, tool_use_id: &str, answers: &Map<String, Value>) -> io::Result<PathBuf>;
```

### `workbench`

```rust
pub enum Edit { …, Newline, Up, Down, Undo, Submit }
pub fn line_edit(key: &str, ctrl: bool, alt: bool, shift: bool) -> Option<Edit>;
impl Line {
    pub fn undo(&mut self) -> bool;
    pub fn line_col(&self) -> (usize, usize);
    pub fn up(&mut self); pub fn down(&mut self);
}
pub const UNDO_KEPT: usize = 200;
impl Bench { pub fn with_question_mut(&mut self, id: &SurfaceId, f: impl FnOnce(&mut Question)) -> bool; }
```

### `pane/bench.rs` — the wiring

```rust
impl TerminalView {
    pub fn channel_events(&mut self, events: Vec<channel::Inbound>, cx: &mut Context<Self>);
    pub fn bench_beacon(&mut self);
    fn bench_hook_press(&mut self, id: &SurfaceId, nav: usize, cx: &mut Context<Self>);
    fn bench_interrupt(&mut self, cx: &mut Context<Self>);
    fn bench_copy_draft(&mut self, cut: bool, cx: &mut Context<Self>);
    fn bench_recall(&mut self, up: bool, cx: &mut Context<Self>);
    fn journal_out(&self, record: &channel::Outbound);
}
// TerminalView gains: wb_channel: channel::State, wb_asked_by_hook: bool,
// wb_sent: Vec<String>, wb_recall: Option<usize>, wb_beacon: Option<(bool, u64)>
```

## Call stack

**A key on the workbench face.** `pane.rs::on_key` → `keylayer::route` →
`Layer::Bench` → `pane/bench.rs::bench_key` → (talking) paste chord /
`ctrl+c` copy / `ctrl+x` cut / `ctrl+g` interrupt / recall / `line_edit` →
`Line::apply` or `Line::insert` → `composer_follows` → repaint. **No call to
`keystroke_bytes`, `send` or `bench_keystroke` on this path.** The one bench
keystroke left is the image-paste chord (`0x16`), which lets the agent read
the clipboard itself.

**SEND.** `bench_key` (`Edit::Submit`) → `bench_send` → `channel::say_bytes`
(bracketed from the replica `TermMode`) → `journal_out(Say)` → history →
`bench_deliver` → `bench_may_write` ? `write_through` : `wb_queued`.

**A question arriving.** `main.rs` surface loop → `Feed::sweep` →
`Feed::tail_inbound` → `Workspace::deliver_surfaces` →
`TerminalView::channel_events` → `State::take` → `Effect::Present` →
`TerminalView::present` → `Bench::apply`. The 120 ms scan ORs
`has_open_question` into `needs_input`.

**A press on a hook card.** `bench_hit` → `Hit::Choose(i)` → `bench_choose`
(option → nav index) → `bench_act(Choose, nav)` → intercept: `wb_channel.owns`
→ `bench_hook_press` → `State::press` → `Press::WriteAnswers` →
`journal_out(Answer)` → `surfacefeed::write_answers`; or `Press::Keys` →
`journal_out(Keys)` → `bench_deliver`; or `Press::Sentence` → the TDSP §7 line
→ then `State::round_surfaces` → `present` for each card.

**The screen reader meeting a hook question.** `sweep_live_questions` →
`live_questions` → `screenread::question_on_screen` → `State::matching` →
`State::saw_cursor` + `Bench::with_question_mut` (cursor only) → no second
card.

**The marker.** `sweep_live_questions` → `bench_beacon` (agent panes, ~1 Hz,
or at once when open/closed flips) → `surfacefeed::write_marker`.

**The hook side.** Claude Code → `td-agent-hooks` (stdin JSON) → `emit` under
`flock` → on `PreToolUse`/`AskUserQuestion`: `fresh()` → `waiting` → poll
`answers/<key>.json` → `released` → stdout decision JSON, or exit 0.

## Test plan

Every one of these fails on `origin/main` today, or did not exist.

**`channel.rs`** (unit, no I/O)
- `every_inbound_type_parses_and_an_unknown_one_is_kept`
- `a_say_is_one_bracketed_paste_and_a_return_or_flat_when_the_terminal_cannot`
- `an_answer_takes_the_first_open_road`
- `the_marker_holds_only_on_a_fresh_workbench_face`
- `a_round_becomes_one_card_per_question_with_the_round_drawn_on_each`
- `a_complete_round_answers_with_labels_keyed_by_question_text`
- `a_press_records_first_then_takes_the_road_the_hook_left_open`
- `once_the_picker_has_painted_a_press_drives_it_with_keys`
- `the_harnesss_own_answer_closes_the_round_and_names_what_was_chosen`
- `a_reply_is_presented_only_when_the_agent_presented_nothing_itself`
- `unknown_records_are_counted_and_the_pane_knows_it_has_been_heard_from`
- `an_outbound_record_carries_its_road_and_the_keys_road_is_never_silent`
- `a_tool_use_id_becomes_a_file_name_that_cannot_walk`

**`surfacefeed.rs`** (scratch directory)
- `the_inbound_journal_is_read_by_offset_and_a_half_written_line_waits`
- `the_marker_and_the_answer_file_are_whole_when_read`

**`workbench.rs`**
- `shift_enter_is_a_line_and_enter_alone_is_the_send` — #614's fix, as a table
- `a_draft_takes_its_changes_back_in_order`
- the existing conventions table, now with the shift column

**`surface.rs`**
- the origin test gains `Origin::Hook`: labelled, more precise than `Derived`, attributed

**`scripts/td-agent-hooks.test.mjs`** (drives the real script)
- outside a pane, nothing; prompt/reply/notify records; a question with no
  bench released at once; the bench's answer becomes the pre-answer; a stale
  marker releases within seconds; `TD_ASK_WAIT_S` bounds the wait; terminal
  face and future clocks are not a bench; the hostile id names a safe file;
  `PostToolUse` forwarded whole; garbage in, nothing out.

**Guards that had to keep passing** (all source scans):
`no_bench_method_is_defined_in_pane_rs`,
`a_renderer_contains_no_decisions`,
`nothing_in_the_bench_half_flips_the_pane_off_the_bench`,
`the_dial_types_beside_the_draft_rather_than_through_the_composer`, and the
comments-board scan in `pane.rs` that requires the note routing above the
`if talking {` branch.

**Not tested here, and said so:** the gpui key path end to end (the harness
runs the binary without a window, #497/#586); the multi-select answer shape
against a live harness; Codex and Gemini.

## Least confident decisions

Carried from `02-architecture.md` and numbered the same there; the ones that
are about THIS layer:

1. **`State` lives on `TerminalView`** (`wb_channel`), not on `Bench`. It is
   pane-and-process state — rounds belong to the agent in this pane — and
   `Bench` is about to gain a conversation key from the tenancy pane. Keeping
   the two apart means the move of `Bench` out of the pane does not drag the
   channel with it before anyone has decided it should.
2. **`round_surfaces` re-presents the whole round after every press.** Cheap,
   and it is what keeps every card's progress strip honest. It relies on
   `Bench::apply` treating an equal re-present as no change, which it does.
3. **The reply surface's id is `reply-hook-<now_ms>`.** Unique by the clock,
   which two replies inside one millisecond would defeat; a per-pane counter
   would be safer and is one line if it ever matters.
4. **`tail_inbound` treats a shrunken journal as rotated and restarts from
   zero.** A journal is append-only by contract, so this is the recovery for a
   contract broken by hand, and it can re-deliver records once. Idempotent
   presents make that harmless for questions; a re-delivered `prompt` would
   re-caption with the same words.
