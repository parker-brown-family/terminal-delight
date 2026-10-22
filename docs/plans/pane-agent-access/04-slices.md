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
4. **The rail by risk, and bypass lines.**
   - Every `Ask` carries a `Severity`, and `rail_ink` follows it.
   - Add `AskLine` with the 1/3/10 count picker and `grant_bypass`.
   - Enforce one pending ask per pane.
   - Proof: `risk_matches_the_rail_table`, `rail_ink_follows_risk_for_access_asks`, `copy_is_not_a_wire_change`, and `one_pending_ask_per_asking_pane`.
5. **Copies.**
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
