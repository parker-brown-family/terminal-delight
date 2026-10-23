# Product: the War Room, first step

Status: **APPROVED 2026-10-22** — eight concurs on `reports/2026-10-22-the-war-room.html`, with two amendments:

- **The door is the existing agent counter**, top right, not a new glyph in the bottom-right tray. Parker: *"we will NOT put this glyph in teh bottom tray - the other agent agreed to put their artifacts glyph down there. This will USE THE EXXXXISTING agent wall rail (don't change look - just change where it goes) -- the agent wal will be an easter egg now... stil runs and is accessible (for now) but hidden with its hotkey."* The counter keeps its look (robot glyph, then working, blocked, error, done, idle and unknown cells); its click opens the War Room. The agent wall stays reachable by its hotkey only. The artifact rail gets a new scroll glyph in the bottom-right tray (Parker to that pane, 19:07).
- **Rail cards are coloured by risk.** On the two new cards: *"COlour coding needs to distinguish the severity or seriousness or risk of the ask."* The colour says how serious the ask is, not which kind it is.

## Problem

Parker, on the second brief (2026-10-22):

> Awesome - then this becomes the WARROOM - BOARDROOM - AGENT MANAGEMENT SURFACE

> multi-agents are already a thing with Claude … and Codex as well … Spinning up a five-agent workforce to complete a task, that's already a thing. What we're looking at here is a more refined management and observability chart. The long-term goal with this is to be able to have your managing agent spin up a new Terminal Delight with a worker agent. … resource allocation and task on targetness are going to be vitally important. A manager might only have access to agents of a certain model level or lower … A certain number of agents, or maybe a certain token budget. … This first step will be simply to allow it to be observability and permission allowance on the chart itself. We'll make the next steps to be first allocating. It's going to have to be simultaneously. … a whole big push to be able to allocate resources to managers and also allow managers to spin up panes and tabs or even entire projects.

> Let's aim high and start small.

Put plainly: one window holds dozens of agents (34 in this one, measured 2026-10-22). Today, finding out who is working on what means reading their screens one at a time, and an agent learns about another agent's work when Parker pastes it across. The release in progress lets agents prompt each other. Nothing yet says which agent may ask or task which, and nothing shows what a question will cost before it is asked.

## Aim high: the push, later and all at once

Managers start their own workers in new panes, tabs or whole projects, inside a budget: which models they may use, how many agents, how many tokens. Work is handed over along lines (assign and scoped assign). These arrive together because a manager that can start workers without a budget should not exist even for a day (*"It can't be up for a second"*).

Also waiting, unsolved: a long-lived coordinator's own conversation grows for days. Parker's options are handoffs, someone else's harness (which brings its own model access), or a harness of our own.

## Start small: this feature

1. **The War Room**, opened from the existing agent counter at the top right (amended; first drawn as a new glyph in the bottom-right row). Seats are agents you placed; lines are what you drew between them.
2. **Observability.** Each seat shows its state (working, needs you, done, idle, or unreadable), its program and model, how much it is carrying, and its latest summary for you. The size it carries is also what one question to a copy of it would read.
3. **Permission.** You place seats, draw lines, and set how far each line reaches: see, read or ask. Off-limits and read-only panes cap every line into them. Agents may ask you for a bypass line on the rail; it grants three prompts by default.
4. **Asks, cheapest first.** An agent's question is answered first from what can already be seen: the other agent's latest summary, its note, its recent tool calls and its screen. A read-only copy of the other agent starts only when the asker says that was not enough. A copy that would read more than a limit waits for your press.

## Success metric

- **Zero** prompts between agents without a line, counted from Terminal Delight's own record of asks.
- **Tokens per answered ask**, split by whether a copy was needed. No baseline exists, since agents cannot ask each other yet. For scale: the cheap reads cost a few hundred tokens each; a copy of the median Claude pane today reads about 360,000 tokens on every model call it makes.

## Announcement

Terminal Delight has a War Room. Open it from the corner of the window and you see every agent you have placed: who it answers to, what it is doing right now, which model it runs and how much it is carrying. Draw a line from one agent to another to let it watch or ask, and nothing passes between agents without one. When an agent asks another a question, Terminal Delight first answers from what it can already see, and only if that is not enough does it start a read-only copy of the other agent, telling you the price first if it is large. The agent being asked is never interrupted, and its conversation never changes.

## Screens

In `mockups/` and drawn at size in the brief:

- `war-room.html` — the panel: seats, lines, states, sizes, off-the-chart count.
- `seat.html` — one seat opened: its summary, note, lines, and what an ask would read.
- `rail-cards.html` — the two cards that reach your rail: a bypass request, and an expensive question.

## Questions for this gate — all concurred 2026-10-22

1. The first step includes asks through read-only copies; assign and scoped assign wait for the push.
2. An ask answers from the cheap reads first; a copy starts only when the asker asks again.
3. A copy that would read more than 400,000 tokens needs your press. *(least sure: the number)*
4. A copy runs on the worker's own model, so it reuses what the worker has already paid to read.
5. A bypass grants three prompts unless you pick another count at the press.
6. Roles come from the lines: a seat with no manager is an orchestrator; a seat with reports is a manager.
7. The chart groups seats by the left bar's projects.
8. Agents see only their own lines, never the whole chart.
