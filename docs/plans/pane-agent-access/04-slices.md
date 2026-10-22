# Slices: the War Room, first step

Status: **folded into building.** Gates 2 and 3 came back as bare approvals in a row (eight and seven concurs, no notes; Parker: "Great!", then "go!"). By the playbook's rule this slice plan is written and building starts without a separate approval round. Any slice that turns up a decision the gates did not make stops and asks.

Branch `feat/war-room` off `dev/0.5.0` at `d17d0661`. Worktree `~/Work/td-authority-lab/war-room`, target `~/Work/td-authority-lab/war-room-target`. That target is a reflink of the lab's clean-target, and it builds against the lab's `zed-upstream`. One pull request into `dev/0.5.0`. Nothing is installed over the live window without Parker's word.

Every slice runs the CI gates from `app/` and leaves the suite green. The gates are `cargo fmt -- --check`, `cargo check --locked`, `cargo clippy --locked -- -D warnings`, and `cargo test --locked --no-fail-fast`. Each slice that adds a security rule also makes its planned break and watches the named test go red.

1. **[DONE 2026-10-22, 9f088c6a] Tracer bullet: the rules, and every exposure site choosing.**
   - Add `warroom.rs`: `SeatId`, `AgentAccess`, `Rung`/`RungWord`, `Line`, `Chart`, `Reach`, `reach`, `may_write`, roles, risk, `Refusal`.
   - `PaneInfo.exposed` is deleted. Every site says which reach it means.
   - The saved leaf gains `seat` and `agents`, and the window gains `lines` and `ask_press_tokens`.
   - With no seats and no lines, behaviour is identical to today, and a test pins that.
   - Proof: the `warroom.rs` rule tests, `off_limits_wins_over_an_agents_word`, `a_state_file_without_lines_reads_as_no_chart`, `a_seat_survives_a_respawn`, and the planned break *let an unseated pane be asked*.
2. **[DONE 2026-10-22] The War Room panel and its doors.**
   - The counter and the `…` row open it, and the wall keeps only Ctrl+Shift+A.
   - The five switches move, drawn by one function.
   - A person can seat and unseat a pane and draw, change or erase a line, by click then click.
   - Seat facts come from what exists: state, size and model (Claude), and the latest summary and note.
   - The read-only position is in the right-click menu.
   - Strings are written in all nine languages.
   - Proof: the door tests, `the_switch_row_is_drawn_by_one_function`, `only_the_war_room_and_the_rail_draw_lines`, `latest_response_ignores_the_selected_card`, and screenshots of a scratch window.
3. **[DONE 2026-10-22] Cheap asks.**
   - Add `ask_agent` (cheap), `ask_answer`, `askrecord.rs`, and the untrusted label.
   - `list_panes` follows lines and counts what is hidden.
   - Proof: the `mcp.rs` tests, the record tests, and the planned break that lets an unseated pane be asked, run again through the verb.
4. **[DONE 2026-10-22] The rail by risk, and bypass lines.**
   - Every `Ask` carries a `Severity`, and `rail_ink` follows it.
   - Add `AskLine` with the 1/3/10 count picker and `grant_bypass`.
   - Enforce one pending ask per pane.
   - Proof: `risk_matches_the_rail_table`, `rail_ink_follows_risk_for_access_asks`, `copy_is_not_a_wire_change`, and `one_pending_ask_per_asking_pane`.
5. **[DONE 2026-10-22] Copies.**
   - Promote `Transport`, then add `askcopy.rs`: plan, `keep_env`, `check_init`, and the Codex pin.
   - Resolve the conversation as Declared and unique.
   - Price the copy and ask for your press over the limit.
   - Spawn the copy, record the answer, and notify.
   - Read the Codex size and model in vitals.
   - Proof: the `askcopy.rs` tests, the conversation tests, the three remaining planned breaks, and the two by-hand live copies.
6. **Words and docs.**
   - Update `docs/mcp.md`, the docsite MCP page and `docs/security/terminal-input.md`.
   - Change the ctl refusal text.
   - Write a readable review page for Parker.

## Progress

- **Slice 1, done (9f088c6a).** `warroom.rs` is in. `TerminalView.agent_access` replaces the off-limits flag. `PaneInfo.exposed` and `mcp::should_expose`/`may_write` are deleted, and every site states its reach. Seats, lines and the press limit are saved.
  - Gates: fmt, clippy `-D warnings`, and 2,731 tests passing (baseline 2,712), 15 ignored as before.
  - Planned breaks, each made, caught and undone:
    - *unseated pane asked*: `an_unseated_pane_is_read_and_never_asked`, `a_plain_shell_follows_the_window_switch` and the mcp seated test went red.
    - *seated without a line read*: four tests went red.
    - *off-limits ignored*: `an_off_limits_pane_is_hidden_from_every_line` went red.
    - *read-only takes writes*: `may_write_refuses_a_read_only_pane_for_every_write_verb` went red.
  - Two things found on the way:
    - A test asking `get_pane_config` with the wrong target shape had passed its "hidden" half without testing anything. It was fixed before the commit.
    - `list_panes`' wire field `exposed` became `reach`; see docs/mcp.md.
  - Owed: `a_seat_survives_a_respawn` is covered only by the saved-leaf round trip. Both restore paths (`build_node`, `build_node_attached`) copy the saved seat onto the view whatever the attach plan does, but a gpui-level respawn test is still to be written, in slice 2.
- **Slice 2, done.**
  - The counter and the `…` row (`more-mcp`, now labelled with the War Room's name) open the War Room. Ctrl+Shift+A alone opens the wall, and its help row is gone.
  - The five switches are one function, `mcp_switch_buttons`, which both surfaces call.
  - The panel is `war_room_view.rs`. It shows seats in rows by `warroom::layout`, with elbow lines on a canvas. Lines that skip a row run down the column gap, and a bypass is dashed. Label chips are clickable: a click cycles ask, see, read, then erase, and a click on a bypass erases it.
  - A seat card shows the agent's latest summary (`Bench::latest_response`), state, model, size (from vitals), note, and lines in and out. Off-chart agents each get a Seat button.
  - The right-click menu has three position rows, with the current one marked, and a READ-ONLY badge. All strings are in nine languages.
  - `TD_WAR_ROOM_DEMO` opens the War Room for a rig. Agent sizes are refreshed while it is open.
  - Tests: `only_ctrl_shift_a_opens_the_wall`, `the_switch_row_is_drawn_by_one_function`, `only_the_war_room_and_the_rail_draw_lines`, `latest_response_ignores_the_selected_card`, two layout tests, the rewritten door test, and the person-only setter scan moved to `set_agent_access`.
  - Gates: fmt, clippy `-D warnings`, 2,737 tests passing, 15 ignored.
  - Photographed in a hidden window with stand-in agents (`evidence/rig/`). Shots are in `reports/2026-10-22-war-room-evidence/`.
  - Found by the photographs, fixed before commit:
    - A line two rows down crossed the seat between and covered its label.
    - The bypass label sat on a shorter line's label.
- **Slice 3, done (23643370).**
  - `ask_agent` answers from the cheap reads, decided by `warroom::decide_ask` in the window's `ask_bridge`.
  - `askrecord.rs` keeps each ask's length and fingerprint, never its words. `list_panes` gives a `hidden` count, and the War Room shows the ask numbers.
  - Planned break *drop the line check from `decide_ask`*: `an_ask_is_refused_by_name_in_order` went red.
  - End to end, in the hidden rig, the stand-in release lead asked from inside its own seated pane through `ctl --workspace special:warroomrig mcp from <session> <pane>`, the door a real agent's relay uses (`evidence/rig/asks-2026-10-22.out`):
    - backend lead: answered, along its ask line;
    - auth refactor: answered, via the bypass (read-only caps at ask);
    - docs pages: refused `no_line` (a read line);
    - scratch: refused `no_line` (unseated).
  - The record held four lines, with fingerprints and no words (`evidence/rig/asks-record-2026-10-22.jsonl`). The bypass was not spent by cheap answers.
  - The War Room shot (`reports/2026-10-22-war-room-evidence/war-room-asked.png`) shows "2 answered from what was visible · 0 by a copy · 0 with no line".
  - Found: `list_panes`'s MCP-off error still says "the robot button on the mother bar". It is fixed in slice 6 with the ctl text.
  - Flake: `host::owning::a_pane_knows_which_session_and_pane_it_is` failed once under a full run and passed 5/5 alone, with host.rs untouched here. File an issue.
- **Slice 4, done (3aad9bdd).**
  - Rail asks are inked by `coordination::Severity`, using the theme's ANSI 2/4/3/1 through `rail_risk_slot`, and the glyph and name lead the card.
  - `access::Change::AskLine` (`ask_line`) asks for a bypass. The card's 1/3/10 picker sets `Ask.prompts` (3 preselected), and Yes calls `Chart::grant_bypass`.
  - One pending ask per agent.
  - request_access's description and schema list `ask_line`, and now say a second ask is refused rather than replacing the first.
  - Gates: fmt, clippy `-D warnings`, 2,749 tests passing, 15 ignored.
  - Planned break *drop the pending check*: `one_pending_ask_per_asking_pane` went red. The first attempt at this break matched the test's own copy of the line and changed nothing. It was redone on the production line only, as the gates hub warns.
- **Slice 5, done.**
  - `askcopy.rs` plans and runs one read-only copy:
    - the measured switches;
    - an environment allowlist (never `TD_*`);
    - the conversation, which must be named by its own agent (`Bond::Declared`) and by no other pane;
    - Claude's server and tool-list checks;
    - the Codex version pin.
  - The model list's `Transport` is shared: `start_with` takes a deadline, an output cap and the noun its errors use, so the model list's own messages are unchanged.
  - `ask_agent` with `copy: true` returns an ask id and a stage. The new verb `ask_answer` reads it back for the asking pane only, labelled untrusted.
  - A copy at or under the press limit starts at once. One over it, or of unknown size, puts a Copy card on the asker's rail row, coloured by `warroom::risk`. Yes starts the copy; No declines it.
  - `Change::Copy` has no wire name, so no agent can raise one.
  - The record keeps every stage: priced, copy_started or pressed, answered, declined, refused, failed. Each stage carries the question's length and fingerprint, never its words. The book drops the words once the copy is done.
  - The vitals keep the raw model id (`claude-opus-5-5[1m]` from the cost record when the session is on the 1M tier). They now read a Codex rollout's last call size and model. Fatigue and relevance stay unmeasured for Codex, and its call is NO READING.
  - `warroom.rs` lost its dead-code allowance; the one method nothing called (`Chart::is_empty`) is gone.
  - Gates: fmt, clippy `-D warnings`, 2,776 tests passing, 17 ignored. The 17 are the earlier 15 plus the two live copies. Earlier slices reported 15 here, which matched, because a grep dropped test groups with no passes.
  - **Departures from the program design, each measured or argued:**
    1. *Claude's tool list is checked after the question is sent, not before.* Claude prints `system/init` only once a user message arrives. Measured: init came 1.3 s before the first word of the answer, after hooks. Servers are checked before the question through the `mcp_status` control request. The tool list is checked the moment init lands, and the copy is killed ahead of any model output. A stand-in that comes up with Write and then leaves a mark two seconds later is killed before the mark appears: `a_copy_that_comes_up_wider_is_killed_before_it_can_act`.
    2. *The second Claude check watches results, not attempts.* A copy remembers the worker's tools and reaches for them. Measured: Write, then Bash, then ToolSearch, each answered "No such tool available" as an error. Killing on the attempt failed the first live run, and would fail almost every copy of a working agent. A tool outside the grant that returns a non-error result stops the copy. A server-side tool use stops it on sight.
    3. *Pricing moved from the War Room's readings to the MCP connection thread.* The vitals sweep runs only while the War Room or the MCP menu is open, which is a cost decision recorded in the sweep's own comment. So most asks would have priced as unknown, and the rest at a reading minutes old. The connection thread resolves which conversation the pane holds, as `pane_events` already does, and reads the size and model of the transcript the copy would fork. An unprovable conversation is refused at the ask, not after a press. When the copy runs, its conversation must still be the one priced.
    4. *Nothing is typed into the asker when a copy finishes, and nothing is pushed.* A line typed there is signed with the session's tag as its person's, and a copy finishing is not something a person did. The asker reads the answer with `ask_answer`. After a press, the rail's own signed answer line names `ask_answer` and the ask id. A pushed notification is still owed.
    5. *One copy at a time per asking pane* (`busy`). An agent with a drawn ask line could otherwise start copies in parallel, each reading the whole of another agent's context.
    6. *The question's first 40 words are shown on the copy card,* because the person is deciding what to pay for.
  - Proof:
    - `askcopy.rs`: both plans carry every measured switch; the environment tests; the init, servers and no-list refusals; tools that run outside the grant; the unproven Codex version; guessed and shared conversations; the price read from the transcript; answers keep "not reported" apart from zero; the wire-level kill.
    - `warroom.rs`: the press rule, `ask_answer_only_answers_the_asker`, one copy at a time, the book keeping open asks.
    - `mcp.rs`: `ask_agent_copy_over_the_limit_waits_for_a_press_and_ask_answer_reads_it`.
    - `access.rs`: `copy_is_not_a_wire_change`, the card's words and answer line, `a_copy_starts_only_under_the_limit_or_on_a_press`, and the agent-facing scan extended to `answer_copy`, `start_copy` and `grant_bypass`.
    - `attention.rs`: `a_copy_row_says_it_starts_a_copy_and_names_ask_agent`.
    - `vitals.rs`: `a_codex_rollout_gives_its_last_call_size_and_model`.
  - Planned breaks, each made on the production line, caught, and restored byte for byte:
    - *`keep_env` keeps everything*: both environment tests went red.
    - *`TD_TAG` added to the allowlist, with the `TD_` belt removed*: `the_copy_environment_never_carries_td_variables` went red.
    - *`check_init` skips servers*: `an_init_with_any_mcp_server_is_refused` went red.
    - *a copy accepts `Bond::Sole`*: `a_guessed_conversation_is_refused_for_a_copy` went red.
    - The first attempt at the `TD_` break only disabled the belt. It changed nothing, because no allowlist entry starts with `TD_`.
  - Live copies, run by hand (`evidence/rig/copies/live-copies-2026-10-22.out`):
    - Claude on Haiku remembered its seed's code word, tried Write and Bash, was refused both, and wrote nothing. It read 167,120 tokens for $0.012.
    - Codex 0.162.1 remembered its seed's code word and read 213,714 tokens. It wrote nothing and did not reach the web. It also declined to try, citing the read-only framing, so its sandbox was exercised by the morning probes and not by this run.
  - End to end, in a hidden window (`evidence/rig/copies/`, shots `war-room-copy-card.png` and `war-room-copy-after-press.png`):
    - A compiled stand-in `claude` holds its transcript open and answers the copy protocol, because a copy is started from the agent's own `/proc/<pid>/exe`.
    - With the press limit at 300k, the release lead asked two seats for copies through `ctl mcp from`, the relay a real agent's MCP calls go through.
    - Backend lead: priced at 290,213 tokens, started at once, answered.
    - Auth refactor: priced at 360,092, waited on the rail as a caution card. The rig pressed `y`, and it answered.
    - Both copies got the measured switches and the worker's model, and no `TD_` variables. The record holds six lines with no words.
  - Found by the photograph and fixed: the card first read "Asks you to change its access" and named `request_access`. It now reads "Asks you to start a read-only copy" and names `ask_agent`, in all nine languages.
  - Found on the way, my own: the rig first waited for the word "done", and the stand-in copy's answer contains "done". The rig tore down the window under the second copy, which looked for a while like the window closing a pipe it did not own.

