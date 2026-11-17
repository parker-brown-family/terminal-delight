# Status: High trust agentic (an agent starts its own team)

Difficulty, submitted before any code: **6/10**. It bends the Threshing
Floor's one hard rule, that only a person seats a pane or draws a line, but
behind a switch that is off by default, on an unpushed branch, and a wrong
call shows the first time the verb is used. One combined plan, this page.
Post-hoc: **6**.

Approval: Parker stated the product in the request (2026-11-15) and said to
build it: *"Better to have an agent that the human talks to and to tell that
agent: SPIN UP THIS TEAM CONFIGURATION"*, *"a SETTING that says: HIGH TRUST
AGENTIC"*, *"Let's get this next big push into the new branch"*. The design
calls below were made without him and are his to overturn.

Branch `feat/high-trust-agentic`, stacked on `feat/multibox`. Local only.

## What it is

With the switch on (Settings › Sharing & access › High trust agentic), an
agent in a pane may call `start_team`: a new tab opens beside its own, up to
four agents start in it with the program, model, effort, permissions and
first prompt it names, each is seated in a new org named after the team, and
the coordinator gets a line to each (prompt unless it asks for less). It then
uses the verbs that already exist: `prompt_agent`, `ask_agent`,
`prompt_status`.

Code: `app/src/team.rs` (the rules and the request), `app/src/team_view.rs`
(the window), `team_seat_and_line` in `threshing_floor_view.rs` (the one
place an agent's request seats a pane or draws a line), `start_team` in
`mcp.rs`, `Line::team` and `prompt::header_team`.

## Calls made without Parker

| # | Call | Why | Undo |
|---|---|---|---|
| 1 | The switch is per window and off by default, beside the other agent-access switches. | Every other agent-access switch is per window; a machine-wide one would switch on windows nobody was thinking about. | `McpConfig::team`. |
| 2 | (Superseded by 11 on Parker's note, 2026-11-15.) One level deep: an agent a team started cannot start a team. | His words: an agent with less than a person's standing starting agents needs thinking "VERY CAREFULLY". The cheap, safe answer is no. | `team::decide`, the `is_member` arm. |
| 3 | At most 4 agents a team, 8 agent-started agents live per window. | One tab holds four panes; two full teams is already a lot of tokens. | `MAX_MEMBERS`, `MAX_LIVE`. |
| 4 | The coordinator's line defaults to prompt, and it may ask for less (see, read, ask). | He said "running prompt (or whatever level of permission)". | `team::parse`. |
| 5 | Lines are downward only and marked: coordinator to its own members. No line back, none between members, none to panes already there. | The smallest grant that makes a team work. A reviewer that needs to ask the coordinator gets that line from a person. | `team_seat_and_line`. |
| 6 | The team's tab opens in the background, beside the coordinator's. | An agent starting agents must not move the person off what they are doing. | `team_start`. |
| 7 | A prompt along a team line tells the worker the truth: its coordinator started it and a person switched the mode on. It does not say "a person drew a prompt line". | It would be false. | `prompt::header_team`. |
| 8 | An agent cannot switch the mode on. The control socket refuses, as it refuses every other global switch from an agent. | The whole wall is that a person turns this on. | `McpPolicy::refusal`. |
| 9 | Model and effort are passed to the program as words, not checked against a catalogue. | The catalogue loads late and differs per machine; a wrong word shows in the pane at once, where the person can see it. | `team_request`. |
| 10 | Teams on another machine are not offered yet. | The far launch path exists (the composer's ssh line) but a far member's line would be a far line with a different owner. Its own design. | n/a |
| 11 | Only a pane a person promoted to orchestrator may start a team; the switch alone lets no agent start anything. Promotion is a right-click row on the pane's Floor card, kept by seat. | Parker on the first version: "this requires FLAGGING or PROMOTING orchestrators... otherwise chaos and token burn compounding is possible". He also asked for more trust for the coordinator: that trust now sits on the promoted pane, which may start several teams and loop its workers. A promoted team member may start a team, because a person chose that. | `team::decide`, `team_promote`. |
| 12 | The counts stay at 4 a team and 8 live. | He asked for more trust, not for more agents; the numbers are an agent's. | `MAX_MEMBERS`, `MAX_LIVE`. |
| 13 | A loop is run by the window, not by the agent: one pass at a time through the prompt book, ended by a phrase on a line of its own in the agent's last message, by a cap that is always present (ceiling 50), by a stop, or by a failed pass. | The research's one complaint about looping tools is that the stop cannot be trusted when the looper is the thing running. | `loopcard.rs`. |
| 14 | A person's loop needs no switch; an orchestrator's needs the switch, the promotion and a prompt line, re-checked before every keystroke. An agent may not loop itself. | A loop is a prompt sent again, so it needs what one prompt needs. A self-loop by an agent is the compounding burn he named. | `loop_request`, `loop_allowed`. |
| 15 | A failed pass ends the loop; a reply that could not be read is unknown and the loop runs to its cap. | Retrying on a clock hides what broke. Unknown is not "no". | `Loop::pass_ended`. |
| 16 | Loops are not saved across a restart. | A loop that resumed by itself is a runaway. | `LoopState`. |
| 17 | No existing routing skill is adopted. Ours is a short page in the kit, `app/skills/agent-routing/SKILL.md`, named by `start_team`. | The research found none both widely used and fit for a pane launcher; gateways cannot start a pane and add a key-holding service. Verdict per the house rule: scuttle. | Delete the file and its `FILES` line. |
| 18 | The routing research ran as one headless Codex; the code review is held for the Codex pane Parker pointed at. | No line runs from this pane, so it cannot be prompted; he asked for the research now and for the review from that pane. | n/a |
| 19 | The pane's own right-click menu promotes, as well as the Floor card; one writer. | Parker: "that is the first place I expect to see it." | `PromoteOrchestrator`, `team_promote`. |
| 20 | Anything the window cannot read or decide holds a loop for a person, with its reason; only a person resumes a held loop. Supersedes calls 15's "unknown runs to the cap" and "a failed pass ends the loop". | ChatGPT Pro review: a done phrase is a claim, not a control signal, and carrying on blind was the worst policy. | `Loop::hold`, `loop_request`. |
| 21 | A loop may carry a check command, run by the window in the worker's folder on a claim and at the cap; a claim with a check ends the loop only if it passes; a failure sends one repair pass with its output. Only an orchestrator can set one so far. | The smallest verifier worth having, per the same review; a menu loop has no way to type a command yet. | `run_check`, `check_ended`. |
| 22 | A pause outranks anything that comes back during it: a done claim or a passing check waits for resume. | Codex round two: a passing check overrode a person's pause. | `pass_ended`, `check_ended`. |
| 23 | A team must state a budget of prompts and minutes before it starts; prompts and loop passes along its lines spend it; teams saved before budgets have none rather than an invented one. | First graph step ChatGPT recommended; turns and time are what the window can count for every program. | `team::Budget`, `Teams::charge`. |
| 24 | The next graph step offered to Parker is a verifier join; not started. | Both reviews lean towards it, and it builds on the check and the budget. | n/a |
| 25 | **The loop engine is removed completely**, and calls 13 to 16 and 20 to 22 with it. Kept as `refs/attempts/high-trust/loop-engine` (1958bdb2), out of the review surface. | Parker, 2026-11-16: "we are NOT building a judgement engine"; Terminal Delight's angle is "tooling and observability", "the first pass of a surface to see and navigate to agents". The engine made the window decide when work was done (a done phrase, a check it ran, stall detection) and act as the harness. | Removed by a forward commit, not a revert: two commits mixed loop code with promotion and team fixes that stay. |
| 26 | The team budget (call 23) and the routing page (call 17) are removed, and the advice in the briefing and in `start_team`'s description is cut to facts. | Parker concurred: they inject policy and opinion. The window should give access and show what happened, not tell agents how to spend. | Same commit. |
| 27 | The limits of 4 agents a team and 8 at once stay as fixed numbers. | Parker: "if the person would like to modify - it is open source and they are free to." | n/a |

## Checked

- 5 unit tests in `team::`, one source-pinned guard
  (`an_agent_draws_a_line_only_to_an_agent_it_started_and_only_with_the_switch_on`),
  and the existing who-may-draw-lines test extended to list `draw_team`.
- Rig, through the real MCP relay from inside a pane: refused with the switch
  off, in the sentence that names the switch; with it on, a two-agent team
  (codex and claude, model and effort passed) started in a new tab named after
  the team; a member's own `start_team` refused; the coordinator prompted a
  member along its team line and read "finished" back.
- Promotion (2026-11-15, second round): `only_a_pane_a_person_promoted_may_start_a_team`;
  on the rig, switch on and not promoted refused in the sentence that says
  how, promoted started, a member refused.
- Loops: 9 unit tests in `loopcard::`. The done-line rule, the cap and the
  promotion check were each broken on purpose and their tests failed. On the
  rig, against a stand-in that keeps a readable conversation and mentions the
  phrase mid-sentence on every pass but its third: cap 5 ended at pass 3 as
  said done; cap 2 ended at the cap; a stop during pass 1 of 10 left nothing
  after it (the conversation holds exactly the passes sent); no cap refused;
  a loop along an ask line refused; a person's loop from the Floor menu ran
  and stopped. That run found a withdrawn pass still counted, fixed.

## Not done

- No real Claude or Codex was started: the rig's programs are stand-ins named
  `claude` and `codex`. The launch line is the launcher's own, unchanged.
- No way yet to see or end a team as a team: its tab and org are ordinary, and
  closing the tab ends its agents.
- Teams across machines (call 10).
- A member cannot yet be given a line back to its coordinator by the
  coordinator; a person draws it.
- No real Claude or Codex has been looped. Whether a real agent puts the
  phrase on a line of its own, unprompted beyond the header, is not known.
- A loop has no card of its own beyond the chip on the Floor seat and a
  marquee line at its end; a pane header shows nothing.
- A team budget stated before a team starts, and a verifier required at a
  join: the two builds not chosen.
- The adversarial review by another provider has not run (call 18).
- Second round, 2026-11-16: Codex round one (9 findings) and round two (12 findings, 7 of the first 9 held) all fixed; ChatGPT Pro's design review applied (holds, check gate, stall, keystroke hold, time limit, team budget). 97 related tests. Rig: check failed then repair then verified; pause withdrew a queued pass; a check's background process killed with its group; a two-prompt team refused the third.
- Not done from the reviews: binding a pass to a session incarnation beyond its prompt number; tokens or money in the budget; protecting a check from a worker that edits it; a typed work item and the join; the budget chip is cut off on the card.
- After the removal (2026-11-16): the branch adds 1,775 lines over `feat/multibox`, down from 4,606. Rig: `start_team` refused before promotion, started after promoting from the pane's own menu with no budget, and `loop_agent` is no longer offered.
- Still open, now as observation rather than control: seeing loops an agent's own harness runs, and prompt traffic along a line.
