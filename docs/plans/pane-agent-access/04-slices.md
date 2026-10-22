# Slices: the War Room, first step

Status: **folded into building.** Gates 2 and 3 came back as bare approvals in a row (eight and seven concurs, no notes; Parker: "Great!", then "go!"). By the playbook's rule this slice plan is written and building starts without a separate approval round. Any slice that turns up a decision the gates did not make stops and asks.

Branch `feat/war-room` off `dev/0.5.0` at `d17d0661`. Worktree `~/Work/td-authority-lab/war-room`, target `~/Work/td-authority-lab/war-room-target`. That target is a reflink of the lab's clean-target, and it builds against the lab's `zed-upstream`. One pull request into `dev/0.5.0`. Nothing is installed over the live window without Parker's word.

Every slice runs the CI gates from `app/` and leaves the suite green. The gates are `cargo fmt -- --check`, `cargo check --locked`, `cargo clippy --locked -- -D warnings`, and `cargo test --locked --no-fail-fast`. Each slice that adds a security rule also makes its planned break and watches the named test go red.

1. **Tracer bullet: the rules, and every exposure site choosing.**
   - Add `warroom.rs`: `SeatId`, `AgentAccess`, `Rung`/`RungWord`, `Line`, `Chart`, `Reach`, `reach`, `may_write`, roles, risk, `Refusal`.
   - `PaneInfo.exposed` is deleted. Every site says which reach it means.
   - The saved leaf gains `seat` and `agents`, and the window gains `lines` and `ask_press_tokens`.
   - With no seats and no lines, behaviour is identical to today, and a test pins that.
   - Proof: the `warroom.rs` rule tests, `off_limits_wins_over_an_agents_word`, `a_state_file_without_lines_reads_as_no_chart`, `a_seat_survives_a_respawn`, and the planned break *let an unseated pane be asked*.
2. **The War Room panel and its doors.**
   - The counter and the `…` row open it, and the wall keeps only Ctrl+Shift+A.
   - The five switches move, drawn by one function.
   - A person can seat and unseat a pane and draw, change or erase a line, by click then click.
   - Seat facts come from what exists: state, size and model (Claude), and the latest summary and note.
   - The read-only position is in the right-click menu.
   - Strings are written in all nine languages.
   - Proof: the door tests, `the_switch_row_is_drawn_by_one_function`, `only_the_war_room_and_the_rail_draw_lines`, `latest_response_ignores_the_selected_card`, and screenshots of a scratch window.
3. **Cheap asks.**
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
