# The War Room — launch notes

The War Room lands on `dev/0.5.0` as one pull request: six slices, Parker's four answers from the build page, and a merge with the artifact rail. It is the 0.5.0 work on agent-to-agent communication. Agents can now ask each other questions, and a person decides who may ask whom by drawing an org chart.

Until this build, an agent with MCP on could read every agent pane in the window, and none could ask another anything. Now a person seats agents on a chart and draws lines between them. A line says how far one agent may reach another: find it, read it, or ask it a question. An unseated pane behaves exactly as before, except that no agent can ever ask it anything.

Each section below covers one part of the feature: what you see, what agents can do, what reaches the rail, the defaults, what a copy can and cannot do, the record, what changes under existing scripts, what is not built yet, and how it was proven.

## What you see

**The door is the agent counter**, top right of the window. A click opens the War Room over the panes, and Esc closes it. The `…` menu lists it as well, for a window whose counter is hidden by UI CONFIG. Ctrl+Shift+A still opens the agent wall, and now only the wall. The wall's help row is gone, and the key sheet lists the War Room as *the War Room: who may reach whom, and the window's MCP switches*.

**Its header counts the window**: seats, how many are working, how many need you, and how many agents are off the chart.

**Seats sit in rows.** An agent nobody has seated is listed under OFF THE CHART, *watched under their own positions; no agent prompts them*, with a **Seat** button. A seated agent gets a card showing:

- its agent's latest summary for its person;
- its state, model and size, the size being how many tokens its last call read;
- its sticky note;
- its lines in and out.

The chart arranges seats into rows by their lines, labelled orchestrator, manager or worker. A seat with no lines is labelled seated.

**Draw a line** by clicking one seat, then another. The line arrives at the `ask` rung. Click its label to cycle it through ask, see, read, and then erase. A line that skips a row runs down the gap between columns, so it never crosses a seat. A **bypass line**, granted on the rail, is dashed and labelled *bypass · N left*. Clicking its label erases it.

**The window's five MCP switches** sit in the War Room as well as on the wall: the server on or off, which panes agents may see (agents, or every pane), pushed events, writes, and card images. Both surfaces draw them with one function, so the two can never disagree. Every refusal that used to send people to the robot button or the MCP CONTROL panel now names *the War Room (the agent counter, top right)*.

**The bottom line counts this session's asks**: answered from what was visible, answered by a copy, and answered with no line. The last must stay at zero.

**Each pane has a position**, set from its right-click menu. The menu has three rows, with the current one marked:

| Position | Agents in other panes may | Header badge |
|---|---|---|
| Open (the default) | everything the window's switches and the lines allow | none |
| Read-only | see, read and, along a line, ask; never write to it | `READ-ONLY` |
| Off-limits | nothing at all; it is gone from every agent's view | `OFF-LIMITS` |

The agent inside a read-only or off-limits pane keeps its own-pane verbs: its own note, deliverable, surface and documents. Only a person sets a position. No MCP or `ctl` verb can.

## What agents can do

| Verb | What is new |
|---|---|
| `list_panes` | Each pane carries `reach`: `seen`, `read` or `ask`. Beside the panes, `hidden` counts the panes the caller cannot see, and never names them. |
| `ask_agent` | Asks a pane a question along a line that reaches `ask`. Without `copy`, the answer comes at once from what the window already shows: the agent's latest summary, sticky note, state, model, size and last ten tool calls. Nothing reaches the asked agent. |
| `ask_agent` with `copy: true` | Forks the asked agent's own conversation into a copy that can only read, asks it the question once, and stops it. Returns an `ask_id` and a stage at once. |
| `ask_answer` | Reads a copy's answer back, for the pane that asked and no other. The answer is labelled untrusted. |
| `request_access` with `ask_line` | Asks a person for a bypass line to a seat. The person picks how many copies it allows: one, three or ten. |

The reach rules, in `warroom::reach`:

- **A pane nobody seated keeps the window's switches.** Agents read it as they always have, and never ask it.
- **A seated pane is hidden** from every agent, except along a line from that agent's own seat.
- **Read-only caps a line at ask. Off-limits beats every line.**
- **Only a person draws, changes or erases a line.** The source test `only_the_war_room_and_the_rail_draw_lines` keeps it so.
- **Seats are random ids saved with the pane**, because pane numbers restart when the session host does. Lines are saved in the layout between seat ids.
- **The pushed events cannot tell who is listening**, so they carry only unseated panes. A seated pane's tool calls are never pushed.

An ask that cannot go ahead is refused by name, so an agent can tell its person why:

| Refusal | Why |
|---|---|
| `no_line` | no line from the asker's seat reaches as far as ask, including when either pane is unseated |
| `off_limits`, `not_an_agent`, `own_pane`, `gone` | the target cannot be asked |
| `bad_question` | empty, or longer than 4,000 characters |
| `mcp_off` | MCP is switched off in this window |
| `cannot_copy` | a copy can be made of a Claude or Codex agent only |
| `unproven_program` | this Codex version's switches were never measured (0.162.1 is) |
| `conversation_unnamed` | the agent has not named its conversation, so a copy would be of a guess |
| `conversation_shared` | another pane names the same conversation |
| `busy` | the asker's earlier copy is still running or waiting |
| `tools_wider` | the copy came up able to do more than read, and was stopped |
| `no_such_ask` | `ask_answer` named no copy this pane asked for |

## What reaches you on the rail

Every access card is now coloured by how serious it is, in the theme's own colours, with a glyph and a name leading the card:

| Colour | Means | Examples |
|---|---|---|
| ✓ clear, green | narrows access | put a pane off-limits |
| ◇ caution, blue | reads a little | pushed events on; a bypass for three prompts or fewer within one project; a copy reading up to 600,000 tokens |
| ▲ warning, yellow | wider or costlier | show a hidden pane or every shell; a bypass into another project, into a read-only pane, or for more than three prompts; a copy reading up to 1,000,000 tokens, or of unknown size |
| ! alert, red | writes, or a large copy | turn writes on; a copy reading more than 1,000,000 tokens |

**A copy card** says whose copy it would be, how many tokens it would read, and the first forty words of the question, because you are deciding what to pay for. It reads *Asks you to start a read-only copy* and names `ask_agent`. Yes starts it, No declines it, and `y` and `n` work with the queue open. After a Yes, the asker's signed answer line ends `read it with ask_answer {"ask_id": N}`.

**A bypass card** has a 1 / 3 / 10 picker with 3 selected. Yes grants the line for that many copies. A cheap answer spends nothing, and the line disappears at zero. A bypass never shortens a drawn line that already reaches as far.

**One pending ask per agent.** A second `request_access` from the same pane is refused while the first is on the rail.

## The defaults you agreed to

- **Every copy waits for your Yes.** The press limit, `ask_press_tokens` in the layout, is 0 unless you set one. If you set one, a copy measured at or under it starts at once, and one over it or of unknown size still waits. A size measured as exactly zero waits like an unknown one, because every copy reads at least its own instructions. The window has no control for the limit yet. It is one line in `~/.config/terminal-delight/sessions/<session>.toml`, which the window rewrites as it saves, so change it with that window closed.
- **The asker comes back for the answer.** Nothing is typed into its terminal when a copy finishes, because a typed line is signed as yours and you did nothing. It calls `ask_answer` until a pushed notice exists.
- **The six departures from the program design stand**, as recorded in `04-slices.md`:
  1. Claude's tool list is checked as it arrives.
  2. Tool results are watched, not attempts.
  3. A copy's size is read when it is asked for.
  4. Nothing is typed into the asker.
  5. One copy at a time per asker.
  6. The card shows the question's first forty words.

## What a copy can and cannot do

A copy is made only when it can be made safely, and refused by name otherwise.

- **It forks only a conversation the agent named itself**: a ledger entry, a held descriptor, or its own `--resume`. No other pane may name the same conversation. The conversation is checked again when the copy starts, and it must still be the one that was measured.
- **Claude's copy runs with `Read` alone and no tool server.** Its servers are checked before the question goes in, and its tool list the moment Claude reports one, which is only after the question. That was measured 1.3 s before the first word of the answer. A tool outside the grant that returns a result stops the copy. A copy remembers the worker's tools and reaches for them, and each attempt is refused, so an attempt alone does not stop it.
- **Codex's copy runs in a read-only sandbox with no network**, with no user configuration, no apps and no web search, and only on 0.162.1, the version whose switches were measured.
- **Nothing of Terminal Delight's reaches a copy's environment.** The environment is an allowlist (HOME, PATH, USER, LOGNAME, LANG, TMPDIR, the two agents' config folders, and the XDG_, LC_, ANTHROPIC_ and OPENAI_ prefixes), and never `TD_TAG` or a pane id. It runs in the asked agent's folder on the asked agent's model, so it reads that agent's prompt cache.
- **It leaves nothing behind.** Claude's copy saves no transcript and Codex's is ephemeral. The copy's whole process group is killed when it answers or fails, after five minutes, or past 16 MB of output. An answer is cut at 16,000 characters.
- **One copy at a time per asker.**

What it does not stop: an agent with a shell runs as you. It can edit the layout file, or send keys to the window through the compositor. The War Room is the front door: the one way Terminal Delight offers, labels and answers, so an agent that behaves has a way to ask and you have one place to say yes.

## The record

Every ask, cheap or copy, refused or answered, writes one line per stage to `$XDG_STATE_HOME/terminal-delight/warroom/<session>/asks.jsonl`. A copy's stages are measured, started or pressed, answered, declined, refused and failed. Each line says who asked whom, along which line, at which stage, and what it read and cost. It keeps the question's length and SHA-256, never its words, and never the answer. The window's book of copy asks drops the question's words once the copy is done.

## What changes under existing scripts and habits

- **`list_panes`' `exposed` field is now `reach`**, with three values, and a `hidden` count sits beside the panes.
- **The agent counter opens the War Room, not the wall.** Ctrl+Shift+A is the wall's only door.
- **A second `request_access` from one pane is refused** while its first is on the rail. It used to replace the first.
- **The push feed leaves out seated panes.** A watcher that counts agents from `agent_appeared` sees only unseated ones.
- **The layout file gains** `seat` and `agents` on a pane, `[[lines]]`, and `ask_press_tokens`. A 0.4 window reads a read-only pane as open and does not know seats or lines. From 0.4's code (not tried), its next save writes the layout without them. Nothing is written for older builds.
- **Refusals name the War Room**, where they named the robot button or the MCP CONTROL panel.

## Not built yet

- **A pushed notice when a copy answers.** Until it exists, the asker polls `ask_answer`.
- **Whether a helper agent started by a Codex copy stays read-only** is untested. Codex reports no tool list, and its sub-agent inheritance was not exercised.
- **Keyboard access to the chart.** Seats and lines are mouse-only, and Esc closes.
- **A window-level test that a seat survives a respawn.** The saved-leaf round trip covers it, and both restore paths copy the seat, but no gpui test respawns a pane.
- **A control for the press limit** in the window.
- **Allocation, and agents starting agents.** These were planned as a later push, in one piece.
- **A host test that fails now and then** under a full run, filed as issue 1112, untouched by this work.

## How it was proven

- **Gates on the merged tree**: rustfmt, clippy with `-D warnings`, and the whole suite. The PR records the counts.
- **Planned breaks**: every safety rule added in the six slices and the press-limit change was broken on purpose on its production line. Each break turned its test red and was restored byte for byte, and each is listed by slice in `04-slices.md`.
- **Live copies of seeded sessions**: a Claude copy on Haiku tried Write and Bash, was refused both, and wrote nothing. It read 167,120 tokens for $0.012. A Codex 0.162.1 copy read 213,714 tokens and wrote nothing. It declined to try, so its sandbox was proven by the morning's probes, not by that run.
- **A hidden window with stand-in agents**, with the press limit at 300,000. A copy reading 290,213 tokens started at once and answered. One reading 360,092 waited as a caution card, was pressed with `y`, and answered. Earlier, cheap asks along an ask line and a bypass were answered, and asks with no line were refused `no_line`. The record held one line per stage and no words.
- **A scratch window and the live window** were tried on the merged build before and after the merge. Their results are recorded in the PR and in the install receipt.

## Where it lives

| Piece | File |
|---|---|
| Positions, seats, lines, rungs, reach, risk, refusals, the copy book | `app/src/warroom.rs` |
| The War Room panel, the ask bridge, copies started and answered | `app/src/war_room_view.rs` |
| Planning and running one read-only copy | `app/src/askcopy.rs` |
| The record of asks | `app/src/askrecord.rs` |
| The MCP verbs | `app/src/mcp.rs`, `app/src/mcp_transport.rs` |
| Rail cards and their answers | `app/src/access.rs`, `app/src/attention.rs`, `app/src/main.rs` |
| A Codex rollout's size and model | `app/src/vitals.rs` |
| The reference for agents | `docs/mcp.md`: *Each pane's position*, *The War Room*, *An agent asks, a person answers*, *Asking another agent* |
| The plan, the slices, the evidence | `docs/plans/pane-agent-access/` |
