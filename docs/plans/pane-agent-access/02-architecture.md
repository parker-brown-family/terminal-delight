# Architecture: the Threshing Floor, first step

Status: **APPROVED 2026-10-22** — eight concurs on `reports/2026-10-22-war-room-architecture.html`, no notes, "Great!". The first gate in this feature with no amendment. The approved enforcement core is `02-enforcement.md`; this page places it in the code. Read from `origin/dev/0.5.0` at `d17d0661` and `origin/feat/agent-management` at `017da926`. Line numbers are as on those refs.

## Fit

**The door.** `render_agent_counter` (main.rs:23558) keeps its look; its click sets a new `threshing_floor` flag instead of `mcp_menu` (main.rs:~23605). The `…` menu row `more-mcp` (main.rs:33406) opens the Threshing Floor too, so a window whose counter is hidden by UI CONFIG (`uiconfig::Key::AgentCounter`) still has a way in. The agent wall stays reachable only by Ctrl+Shift+A (pane.rs:7380 → main.rs:5157). Its help row goes (main.rs:33771). The stale "🤖 (mother bar)" row (main.rs:33770) becomes the counter's row. The source test `the_agent_wall_opens_from_the_counter_that_summarises_it` (main.rs:38406) is rewritten to pin the new doors. Agreed with the artifact rail, which takes a new 📜 glyph and leaves the counter alone (`00-status.md`, Coordination).

**The window's MCP switches.** Today they live inside the wall overlay (`mcp-btn-enable`, `-expose`, `-events`, `-writes`, `-images`, main.rs ~30750–30850). The ctl refusal text (ctl.rs:170) points people at "the MCP control panel (Ctrl+Shift+A)". A hotkey-only wall would hide them, so the switch row is drawn by one function and placed in the Threshing Floor's header; the wall calls the same function. The ctl text names the Threshing Floor.

**A new pure module, `threshing_floor.rs`.** It holds seats, lines and rungs, roles derived from lines, `reach(caller, target)` combining window switches, pane position and line, and risk. It has no gpui and is unit-tested, like `hud.rs` and `access.rs`. The read/act split from the superseded plan lives here, taking the caller as an argument.

**`mcp.rs`.** Every exposure site asks `threshing_floor::reach`. The census hides a seated pane from callers without a line. Two new verbs and one changed verb are listed under Endpoints.

**`access.rs` and the rail.** `Change` gains `AskLine` (wire `ask_line`) and an expensive-ask kind. `Ask` gains `risk: coordination::Severity` and, for a line, a prompt count. `rail_ink` (main.rs:24575) colours an Access card by its risk instead of by its lane, and the card shows the Severity glyph. `rail_answer` (main.rs:24127) creates the bypass line or starts the copy. Today every Access card is the same amber (attention.rs:74, main.rs:24575); no severity reaches the rail.

**Seat facts are reused, not collected anew.**
- State: `hud::AgentState`, the counter's own cells.
- Size and model: `vitals.rs`, which already computes the last call's input plus cache read plus cache write (vitals.rs:660, `window_stat` :877) and the model, per shell pid (`Workspace.agent_vitals`, main.rs:4610). This is Claude only.
- Codex size: not computed today. The vendor collector already reads `last_token_usage` (omarchy-agent-usage-codex:319). Vitals gains that read.
- Latest summary: the bench's newest response (`Bench::showing`, workbench.rs:~4414), extracted as `latest_response()` because `showing()` depends on what the person has selected.
- Note: the pane's sticky note.

**A copy launcher, new `askcopy.rs`.** It reuses `model_catalog::Transport` (model_catalog.rs:460): a background thread, its own process group, a bounded reader, a deadline, and kill on drop. That code already launches `/proc/<agent>/exe` with `--print --input-format stream-json`.

**Which conversation is copied.** `paneident::resolve` (paneident.rs:133), accepting only `Bond::Declared`, and only when no other pane declares the same id. Agent-management applies the same rule to deliveries (management_mcp.rs:259). Declared claims are not exclusive (test `two_panes_naming_one_session_both_get_it`, paneident.rs:607; issue 416), so a shared id is refused rather than guessed.

## Endpoints (MCP)

- `list_panes` (changed): a seated pane appears only to callers with a line to it. Each row the caller sees gains `line` (rung, bypass prompts left). The answer gains `hidden: n`.
- `ask_agent` (new) `{pid, question, copy: false}`:
  - `copy: false` returns the cheap answer at once: seat row, latest summary (brief and plain reading), note, last ten tool calls.
  - `copy: true` returns `{ask_id, state: running | awaiting_press | refused, reads_tokens}`.
- `ask_answer` (new) `{ask_id}` returns `{state, answer?, tokens_read?}`. A pushed `notifications/message` also announces each answer. No call waits on a model.
- `request_access` (changed): change `ask_line` `{pid}`. The count is chosen at the press.
- ctl: nothing new. A line is set only by a person, in the Threshing Floor or by pressing Yes on the rail.

## Data

**Saved window file** (`StateFile`, main.rs:2717; leaf main.rs:870):
- `SavedNode::Leaf.seat: Option<SeatId>`: a random id minted when a person seats the pane. It travels with the saved leaf through any respawn. Pane ids are minted by the host and restart at 1 if the host restarts (host.rs:547), so a line keyed by pane id could land on an unrelated pane.
- `StateFile.lines: Vec<SavedLine { from: SeatId, to: SeatId, rung, prompts_left: Option<u8> }>`. An unknown rung word grants nothing.
- `StateFile.ask_press_tokens: Option<u64>`. Absent means 400,000.
- Nothing is written for older builds.

**Ask record**, append-only, one per window session: `$XDG_STATE_HOME/terminal-delight/threshing-floor/<session>/asks.jsonl`.
- Fields: `{id, at, from_seat, to_seat, via: line | bypass, stage, question_len, question_sha, program, model, reads_tokens, tokens_read, cost_usd?}`, where `stage` is one of `cheap | priced | pressed | declined | copy_started | answered | refused | failed`.
- The question text and the answer are not written to disk. The two Gate 1 numbers (unlined prompts, tokens per answered ask) come from this file.

## Flow

1. **The counter's click** opens the Threshing Floor: seats, lines, the five switches, seat facts.
2. **A cheap ask.** `ask_agent` reaches the window through `mcp_transport`. The caller is found by peer credentials and `threshing_floor::reach(caller, target)` must be at least ask. The window builds the packet, writes `cheap` to the record, and answers.
3. **A copy.** The same checks run again, and a bypass line spends one prompt. Then:
   - The conversation is resolved: `Declared` and unique, or the ask is refused.
   - The size comes from vitals. An unknown size is treated as over the limit.
   - Over the limit, the asker's pane holds an expensive ask, the rail shows it inked by risk, and a Yes starts the copy.
   - Under the limit, the copy starts at once.
4. **The launch,** on its own thread:
   - argv per program (External). The working directory is the worker's.
   - The environment is cleared to an allowlist (`HOME`, `PATH`, `USER`, `LANG`, `XDG_*`, `CLAUDE_CONFIG_DIR`, `CODEX_HOME`). Every `TD_*` variable is removed, so a copy cannot read and return the worker's `TD_TAG`.
   - For Claude, read the first message: tools must be a subset of the grant and servers empty, or the copy is killed and the ask refused.
   - Write the question, read until the result or a five-minute deadline, then kill the process group. Write the record and push the notification.
5. **The answer.** `ask_answer` returns it to the asker.

## External

- **Claude Code 2.1.296.** No transcript is left in the worker's folder (`evidence/`, "A copy that leaves no transcript"). The launch line:

  ```
  claude -p --resume <id> --fork-session --no-session-persistence --model <worker's model> --tools Read --strict-mcp-config --permission-mode default --permission-prompts none --input-format stream-json --output-format stream-json --verbose
  ```

- **codex-cli 0.162.1.** It runs only when the binary reports a version whose switches passed the probe, because Codex prints no tool list to check. The launch line:

  ```
  codex exec fork <id> --ephemeral --ignore-user-config --disable apps -c sandbox_mode=read-only -c approval_policy=never -c web_search=disabled -m <worker's model> --json
  ```

- Environment variable names: `TD_PANE_ID`, `TD_SESSION` and `TD_TAG` are stripped from copies. `TD_MCP_SCRIPTED` is unchanged. There are no new variables.

## Questions for this gate — all concurred 2026-10-22

1. The window's five MCP switches move from the agent wall into the Threshing Floor's header.
2. The `…` menu's agent row opens the Threshing Floor; the wall keeps only Ctrl+Shift+A and loses its help row.
3. Lines are keyed by a seat id saved with the pane, not by pane number.
4. A copy is made only of a conversation its agent named itself and no other pane claims.
5. Every access ask on the rail carries a risk (the existing four-step scale with its glyphs), and Caution takes the theme's blue so the steps differ by hue.
6. A copy runs with a cleaned environment and no Terminal Delight variables.
7. Asks are not gated on the writes switch; the line is the permission. *(least sure)*
8. A Codex seat's size is read from Codex's own record; until that lands, every copy of a Codex seat needs your press.
