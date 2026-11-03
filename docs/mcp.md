# terminal-delight as an MCP control surface

terminal-delight can act as an **[MCP](https://modelcontextprotocol.io)
server**: an orchestrating agent connects over stdio and watches a wall of
panes — who is running where, which conversation each agent is in, and what
tools they call — and reacts, **without ever touching a keyboard**. It can also,
behind a second opt-in, **configure a pane's appearance** — dim the brightness,
flatten the warp, restyle the whole wall at once.

This page describes 0.4.0, the current release (true of `main` at `dad844fa`).
What exists only on the 0.5.0 development branch (true of `dev/0.5.0` at
`d9aa472d`) is marked **0.5.0 development:** in the text or *(0.5.0 dev)* in a
heading or table row. The user-facing reference is the docs site's
[MCP server page](https://docs.terminal-delight.brownfamilysports.com/mcp).

In 0.4.0 no MCP tool writes to a terminal. "Writes" permit UI changes:
appearance, notes, deliverables, work objects and documents. They are disabled
until you turn them on. Standard processes retain your account's authority: the
separate control socket has scripted bench-input and command-launch routes
(`ctl bench say`, `ctl adopt --run`) that any same-user caller can use, and the
session host accepts terminal input from same-user clients. MCP switches and
off-limits marks alone do not contain hostile Standard processes.

**0.5.0 development:** `prompt_agent` types into another agent's prompt, only
along a line a person drew at `prompt` in the Threshing Floor. A live window's
control socket refuses the bench, document, adoption and tab-edit commands, and
every MCP switch change, unless the window was launched with
`TD_MCP_SCRIPTED=1`.

## Wiring it up

`terminal-delight mcp` attaches an agent to the window that is already running:
it is a stdio relay the agent's client starts, which carries each JSON-RPC line
to that window's control socket. Register it with the absolute launcher path:

```json
{
  "mcpServers": {
    "terminal-delight": {
      "command": "/home/parker/.local/bin/terminal-delight",
      "args": ["mcp"]
    }
  }
}
```

or with the clients' own commands, which expand `$HOME` before saving:

```sh
claude mcp add --scope user terminal-delight -- "$HOME/.local/bin/terminal-delight" mcp
codex mcp add terminal-delight -- "$HOME/.local/bin/terminal-delight" mcp
gemini mcp add --scope user terminal-delight "$HOME/.local/bin/terminal-delight" mcp
```

The relay finds its window by walking up its own parent processes to the pane
it was started in (`ctl::locate`), so it needs no environment variable. Failing
that it uses the window the session host recorded for `$TD_SESSION`, then the
only window running; with several running it refuses and lists their pids.
`terminal-delight mcp --pid <N>` names a window. Each call waits up to ten
seconds. If the window is replaced mid-session, the relay asks once more where
its session is, reconnects, and says so on stderr.

The relay sends every line as `mcp from <session> <pane|-> rpc <json>`, naming
the caller. A window whose session differs refuses with `-32001`
(`wrong window: … Nothing was read or written.`). The pane lets a pane-scoped
verb default to the caller's own pane. A window from a build older than
`mcp from` calls it an unknown command; the relay then falls back to the bare
`mcp rpc` form for the rest of its run and says on stderr that pane-scoped verbs
will need an explicit pid.

Every reply names its instance (session, window PID and build); check that it
identifies the window you intend to coordinate. `initialize`, `tools/list` and
`ping` are snapshot-independent and answer at once. `tools/call` waits up to five
seconds for the window and otherwise returns `-32000 "terminal-delight UI not ready"`.

A GUI launched by the MCP client itself, with stdin and stdout piped and
`TD_MCP` set, serves MCP directly on its own stdio. That is the only transport
that pushes notifications. `TD_MCP` starts the transport and leaves the server
switch off; `TD_MCP_WRITE` turns writes on, and only alongside `TD_MCP`. A script
can also send one call with `terminal-delight ctl mcp rpc '<json>'`.

## Turning exposure on

Exposure ships **disabled** and the policy is operator-controlled from the
**MCP SERVER** section of the agent wall, which the agent counter at the top
right opens (`AGENTS · MCP` in the footer's `…` menu opens it too). The wall's
heading reads `MCP CONTROL — READ ONLY` or `MCP CONTROL — READ + WRITE`. Its
switch row:

- **`server enabled`** — master switch. Off ⇒ the server reports nothing.
- **`expose:`** — `agents only` (claude / codex panes; the safe default, never a
  plain shell) or `all panes`.
- **`stream tool-call events`** — allow tailing each agent's transcript for
  tool-call events (powers `pane_events` and the push feed). On by default.
- **`writes on · agents can restyle`** — a *second* opt-in (off by default) that
  promotes the read-only watch surface to a remote-control one: with it on,
  `set_pane_config` may change a pane's (or the window-level `outer`) appearance,
  and agents may leave notes, declare deliverables, present surfaces and open
  documents. None of it reaches a terminal's input. The `TD_MCP_WRITE=1` env var
  forces it on for an orchestrated `TD_MCP` launch.

The policy is persisted in the session layout,
`~/.config/terminal-delight/sessions/<session>.toml` (under
`$XDG_CONFIG_HOME` when that is set), in its `[mcp]` table — not in the legacy
`state.toml`. A connected client always sees the *current* policy live; flipping
a toggle takes effect on the next call. `terminal-delight ctl mcp on|off`,
`mcp writes on|off` and `mcp expose agents|all` change the same switches from a
shell, and `ctl mcp status` prints `enabled=… writes=… expose=… events=…`.

**0.5.0 development:** the switches move into the **Threshing Floor**, the panel
the agent counter opens instead (the `…` menu has a `THRESHING FLOOR` row too),
labelled `server`, `agents only` / `all panes`, `events` and `writes`. The
control socket refuses all six `ctl mcp` policy changes, in either direction,
unless the window was launched with `TD_MCP_SCRIPTED=1`; `ctl mcp status` still
answers.

### Each pane's position: open, off-limits (and read-only in 0.5.0 dev)

Those switches cover the whole window. To say *not this one*, right-click inside
a pane and choose **Off-limits to agents**. The pane wears `OFF-LIMITS` in its
header, and no agent reads it or writes to it under any policy — it is gone from
`list_panes`, `grep`, `pane_events`, the pushed events and the appearance tools,
and a note, deliverable or surface aimed at it from another pane is refused. The
agent running *inside* it keeps its own-pane writes: it can still present its own
work, declare its own deliverable and post its own note, because the mark hides a
pane from other agents rather than silencing the one inside it. It cannot read
`document_notes` or call `open_document`, which both check that the caller's own
pane is exposed. The same menu row, now **Let agents see this pane**, lifts the
mark. The position is saved with the layout.

In 0.4.0 the window trusts the pane the relay names, which guards against
misrouting rather than impersonation (`docs/decisions/0002`). A write to a pane
the policy hides answers `pane <pid> is not exposed under the current policy`;
one to a missing pane answers `no pane with pid <pid>`.

**0.5.0 development:** the right-click menu holds three positions, the current
one marked: **Let agents see this pane**, **Read-only to agents** and
**Off-limits to agents**.

- **Read-only** — agents may see and read it and, along a line, ask it, but
  nothing from another pane writes to it: no note, deliverable, surface,
  appearance change, bench input or synthetic click. It wears `READ-ONLY` in its
  header.
- The agent inside a read-only or off-limits pane keeps its own-pane verbs, and
  its own-document open and read as well. The ctl connection obtains the peer PID
  through `SO_PEERCRED`; its claimed pane survives only when the current pane
  shell is that process or its ancestor. Forged or unavailable ancestry removes
  own-pane authority.
- Hidden and absent write targets answer alike, `no pane with pid <pid>`.
- In ordinary windows, the control socket refuses every person-acting bench,
  document, adoption and tab-edit command, regardless of caller ancestry. A
  shared daemon or an unknown caller gets no exception. Operator rigs must launch
  their own window with `TD_MCP_SCRIPTED=1`; those windows retain off-limits and
  read-only selection and dispatch checks.
- The MCP gate is `threshing_floor::reach` for reading and
  `threshing_floor::may_write` for writing; the idea is Splinterm's per-pane
  grant, with TD's own shape.

Only a person sets or lifts a position. MCP can propose a change with
`request_access`; it cannot approve one.

### The Threshing Floor: who may reach whom (0.5.0 dev)

A position holds for every agent. The Threshing Floor says which agent may reach which
pane, as an org chart a person draws. Each agent pane can be given a **seat**
(the Seat button beside it), and a person draws **lines** between seats: click
one seat, then another. A line carries a rung, and clicking its label cycles it:

| Rung | The sender may |
|---|---|
| `see` | find the pane in `list_panes` |
| `read` | also read it: `grep`, `pane_events`, `get_pane_config` |
| `ask` | also ask it a question with `ask_agent` |
| `prompt` | also give it work with `prompt_agent`: a turn typed into its own conversation |

`ask` and `prompt` are different channels, not two strengths of one. An ask reads
an agent and leaves its conversation as it was; a prompt is its next turn. A
coordinating agent treats the agents it holds a prompt line to as its sub-agents.

The rules, in `threshing_floor::reach`:

- **A pane nobody seated keeps the window's switches**: agents read it as they
  always have, and **never ask or prompt it**. Neither travels without a line.
- **A seated pane is hidden** from every agent except along a line from that
  agent's own seat. `list_panes` says how many panes the caller cannot see, as a
  `hidden` count, and never which.
- **Read-only caps a line at ask; off-limits beats every line.** A read-only
  pane can be asked and never prompted.
- **A line from before prompt existed is not upgraded.** A line drawn at `ask`
  stays an ask line; prompting needs a person to set the line to `prompt`. A
  bypass granted on the rail is always an ask line.
- **Only a person draws, changes or erases a line.** No MCP or `ctl` verb can;
  `only_the_threshing_floor_and_the_rail_draw_lines` keeps it so. An agent may *ask*
  for a bypass line through `request_access` (below).
- **Seats are not pane numbers.** Pane ids restart at 1 when the session host
  restarts, so a seat is a random id saved with the pane, and lines are saved in
  the layout as `[[lines]]` between seat ids. A rung word this build does not
  know grants nothing and survives a save.
- **A feed that cannot name its caller** (the pushed events, the coordination
  reads) treats every seated pane as hidden.

Each seat shows its agent's state, model and size, its latest summary and its
note, and what its lines reach. The bottom line counts this session's asks:
answered from what was visible, answered by a copy, and answered with no line —
which must stay at zero.

### An offline command with less authority (0.5.0 dev)

Standard terminals keep ordinary account access. For code you want confined,
explicitly start a Restricted command before its code or startup files run:

```sh
terminal-delight restricted run --workdir "$HOME/Projects/example" -- sh -c 'make test'
```

See the docs site's [Restricted commands page](https://docs.terminal-delight.brownfamilysports.com/restricted) for the complete user guide. The canonical selected path must be strictly below the OS account home, with no dot-directory in that path. Hidden files already inside a selected project remain granted content; system trees, hidden roots and external volumes need a dedicated project copy.

The installed `/usr/bin/bwrap` must be version 0.12.0 or newer and support `--bind-fd`, `--ro-bind-fd` and
`--seccomp`; Linux 5.11+ descriptor cleanup and required namespaces must work and a private `/run/user/UID` runtime must exist or the
command fails to start. Runtime overrides must stay inside that protected root. There is no Standard fallback. The command sees the
chosen directory at `/work`, read-only system tools/libraries, temporary home
and tmp, and its own process/network views. It receives no host home, saved TD
state, credentials outside the chosen directory, desktop/native-agent services
or host control sockets. New
connections and addressed sends are blocked too, so a host socket added to the
chosen directory after launch cannot bypass the policy. Only the preconnected TD
channel is available. Programs that close inherited descriptors lose that channel
and cannot reconnect; their ordinary offline work can continue.
`--read-only` grants the project read-only. The directory must contain ordinary
files, directories or symlinks, with no sockets, devices, pipes or nested mounts.
Hard-linked work files and overlaps with protected account or configured state/config roots are refused. Root and set-ID invocation are unavailable in this qualified profile. Validate a dedicated
copy when you do not want to share writable output with privileged consumers.

This first profile supports offline, line-oriented shell/build commands. Child
standard streams are pipes and output is filtered into plain UTF-8 text, so it
cannot invoke terminal clipboard or graphics protocols. TUIs, GUI apps, online
provider credentials and automatic agent handoffs are unavailable. The profile
does not promise semantic safety, complete resource quotas or protection against
malware outside it running with the host user's authority.

`--td-note` optionally grants text notes on the calling pane, proven from host
ancestry. It grants no other target or operation, and the existing MCP switches
still apply. Inside the run:

```sh
terminal-delight restricted call '{"op":"note","text":"Build complete"}'
terminal-delight restricted call '{"op":"status"}'
terminal-delight restricted call '{"op":"request_access","reason":"Need external packages"}'
```

One lifetime access proposal is printed in the run terminal and available in status; it never changes access or creates approval authority. It names the current directory and access, labels the reason as workload-supplied, and explains that changed scope needs a new run. `terminal-delight restricted dismiss RUN` clears it from outside without resetting the proposal budget. The workload cannot dismiss or approve it. This release does not put Restricted proposals on the attention rail. From an outside Standard terminal, use the printed
run ID with `terminal-delight restricted status RUN` or
`terminal-delight restricted revoke RUN`. Revoke acknowledges after an already
accepted note finishes and denies subsequent notes. It does not stop file work
or recall earlier effects. Wider or tighter filesystem/network scope needs an
explicit new run. Exiting ends grants; a restart or replacement pane cannot
inherit them. Restricted is a property of a run, not of a pane: an existing pane never becomes Restricted.

The minimum helper version includes the safe setup-path resolution fix. CI builds the pinned upstream release from its verified SHA-256; TD never downloads helpers during a launch.

A note, terminal output or shared file is an authorized output channel, and it
can still mislead an unrestricted reader that chooses to act on it. The profile
has no automatic handoff operation; it cannot constrain the later actions of a
Standard program reading its authorized output. Secrets already inside the
chosen directory are granted content too.

### An agent asks, a person answers

A refusal about access used to be the end of it. Now an agent can **ask**:
`request_access` puts its ask on the attention rail, as a row in the rail's
first lane, `access` ("Asks you to change its access"). The row says what is
being asked in plain words ("Let agents see “prod-db”"), quotes the agent's
reason, and carries two buttons, **Yes  y** and **No  n**; with the queue open,
`y` and `n` answer the row under the cursor.

There are five things to ask for in 0.4.0, all of them about who agents can see
and what agents may do:

| `change` | Yes does | The person's own control for it |
|---|---|---|
| `open_pane` + `pid` | lifts off-limits from that pane | right-click → Let agents see this pane |
| `close_pane` + `pid` | marks that pane off-limits | right-click → Off-limits to agents |
| `writes_on` | turns MCP writes on | agent wall, `writes on · agents can restyle` |
| `expose_shells` | shows plain shells as well as agents | agent wall, `expose: all panes` |
| `events_on` | shows other agents' tool calls | agent wall, `stream tool-call events` |
| `ask_line` + `pid` *(0.5.0 dev)* | a bypass line from the asker's seat to that seat, for the uses picked on the card (1, 3 or 10; 3 unless changed) | Threshing Floor, draw a line |

Appearance is not on the list. With writes on, `set_pane_config` restyles a pane
and asks nobody — Parker's line was that agents should not be *"blocked by human
decision for simple things like themeing"*.

How it behaves in 0.4.0:

- **Nothing changes until a person presses Yes.** No answers on a timer, no
  answer by default. An unanswered ask is a row waiting, like any other.
- **One ask per pane.** A new ask from the same pane replaces the one waiting.
  An ask dies with its pane. Asks are not saved; a window restart drops them.
- **Nothing to ask is refused before it reaches the rail.** An ask for what is
  already so (`MCP writes are already on`), an `open_pane` for a pane that is not
  off-limits, and an `open_pane` or `close_pane` naming a pid this window does
  not have (`no pane with pid 4412 in this window`) never become rows.
- **The answer reaches the agent as one signed line** typed into its own
  terminal: `[workbench:<tag>] request_access open_pane 4412: yes, pane 4412 is
  open to agents`. It goes through the bench's own path, so it is typed when that
  pane is on screen and held until then. The setting does not wait for the line:
  it changes the moment the person answers, so an agent that retries sees it.
- **It works with writes off**, because `writes_on` is one of the things an agent
  may need to ask for. It needs the server switch on and a pane to call from.

**0.5.0 development:** the card is coloured by how serious the ask is, from the theme's own colours: ✓ clear (green), ◇ caution
(blue), ▲ warning (yellow), ! alert (red). And:

- **One ask at a time.** A second ask from the same pane is refused while the
  first is on the rail (`one ask at a time: your earlier ask is still on the
  person's rail`), so one agent cannot fill the rail or hide an earlier ask
  behind a later one.
- **Opening requests do not disclose whether a pane exists.** A valid
  `open_pane` proposal occupies the same queue and gets the same acknowledgement
  for a hidden, missing or already-open target. The person sees the target's
  name, or its PID marked unavailable. This also prevents a second request's
  pending status from revealing existence. Other changes still refuse an ask
  for what is already so.
- **A Yes applies to the original target.** A pane that closes or is replaced
  while the proposal waits cannot transfer its approval to another pane. A
  request for an unavailable target grants nothing, even if that PID later
  belongs to a pane. The answer after the person's decision reports the outcome.

What can answer an ask is the rail's two buttons and its two keys, and nothing
else. No MCP verb carries an answer (the ask rides `ConfigPatch` as a
`serde(skip)` field, so no agent JSON can put one in either), and no `ctl` verb
reaches the rail: `ctl bench click|hover` drives a pane's bench through that
pane's own handlers. `access::tests::no_agent_facing_surface_can_answer_an_ask`
keeps `mcp.rs` and `ctl.rs` from naming the answering code, and
`an_ask_is_answered_only_by_the_rails_buttons_and_keys` keeps the rest of the
window to those two callers.

**0.5.0 development:** a bypass line spends one use for every copy asked along
it (a cheap answer spends nothing) and disappears at zero. It never shortens a
drawn line that already reaches as far.

### Asking another agent (0.5.0 dev)

`ask_agent` asks a pane a question, along a line that reaches `ask`. It has two
rungs, cheapest first.

**The cheap answer** comes at once from what Terminal Delight can already show:
the agent's own latest summary for its person, its sticky note, its state, model
and size, and its last ten tool calls. Nothing reaches the asked agent, and its
conversation does not change.

**A copy** (`copy: true`) answers what the cheap reads cannot. Terminal Delight
forks the asked agent's own conversation into a throwaway copy that can only
read, asks it the question once, and stops it. A copy re-reads the whole of that
agent's context on every model call, so it is priced when it is asked for, from
the very transcript it would fork:

- **By default every copy waits** as a card on the asker's rail row, saying what
  it would read and the first words of the question, until a person presses Yes
  or No. The press limit, `ask_press_tokens` in the layout, is 0 unless set.
- **A person who sets a limit** lets a copy at or under it start at once. A copy
  over it, or of unknown size, still waits for the press. The window has no
  control for the limit yet; it is a line in the layout file.

Either way `ask_agent` returns an `ask_id` and a stage at once, and the asker
reads the answer later with `ask_answer`. Stages: `awaiting_press`, `running`,
`answered` (with the answer and the tokens it read), `declined`, `refused` (with
the reason by name) and `failed`. Only the pane that asked can read its answer.
When a person presses Yes, the asker's signed answer line names `ask_answer` and
the id; nothing else is typed into the asker.

A copy is made only when it can be made safely, and refused by name otherwise:

- **Only of a conversation the agent named itself** (a ledger entry, a held
  descriptor, its own `--resume`), and that no other pane names.
  `conversation_unnamed`, `conversation_shared`.
- **Only Claude and Codex.** `cannot_copy`. Codex only at a version whose
  switches were measured (`unproven_program`; 0.162.1 today), because Codex
  reports no tool list to check.
- **Claude's copy runs with `Read` alone and no tool server.** Its servers are
  checked before the question, its tool list the moment it reports one, and a
  tool outside the grant that returns a result stops it (`tools_wider`).
  Reaching for one is not enough: a copy remembers the worker's tools, tries
  them, and is refused each one.
- **Codex's copy runs in a read-only sandbox with no network**, no user
  configuration, no apps and no web search.
- **Nothing of Terminal Delight's own reaches a copy's environment** — no
  `TD_TAG`, no pane id — and it runs in the asked agent's folder on the asked
  agent's model, so it reads that agent's cache.
- **It leaves nothing behind**: Claude's copy saves no transcript, Codex's is
  ephemeral, and the copy's process group is killed when it answers, fails, or
  runs past five minutes.
- **One copy at a time per asker** (`busy`).

Every ask, cheap or copy, refused or answered, leaves a line per stage in
`$XDG_STATE_HOME/terminal-delight/threshing-floor/<session>/asks.jsonl` (builds from before the rename wrote `warroom/<session>/`, which is still read and counted): who asked whom,
along what, at which stage, what it read and cost. It keeps the question's length
and SHA-256, never its words, and never the answer.

### Prompting another agent (0.5.0 dev)

`prompt_agent` gives another agent work: the sender's words, typed into the
worker's own conversation as its next turn, the way its person would type them.
The worker answers with everything it already knows. It is how a coordinating
agent treats other agents in the window as its sub-agents, whatever program each
one runs, without a second process or a copy.

It goes only along a line a person drew from the sender's seat at `prompt`. An
ask line, a bypass, a read-only pane and an off-limits pane are refused by name
(`no_prompt_line`, `read_only`, `off_limits`). `prompt_agent` returns a
`prompt_id` at once, and nothing has been typed yet. The window types when the
worker is ready, by these rules (`app/src/prompt.rs`):

- **Only at a settled rest.** The words go in after the worker's screen has shown
  its own idle prompt for five seconds, look after look, with no turn beginning in
  between: a streaming agent's screen can look at rest between two chunks. A
  working worker is waited for, up to ten minutes; a resting or asleep one is woken first, without taking the person's
  focus. A pane showing a permission prompt, a question, a dialog, an error, or a
  screen nobody can read is never typed into. A
  worker woken from sleep is a new process holding its old conversation, and its
  prompt says so under the header: background shells, permissions granted for
  that session only, and running loops are gone (`prompt::WOKEN_NOTE`).
- **Never over a person.** A pane its person typed in during the last minute,
  and one showing a picker the workbench's dial raised, waits. The clocks are
  slow on purpose: the work runs while its person is away.
- **As one bracketed paste.** Every control character but newline and tab is
  taken out first, so nothing in the words can close the paste early or press a
  key. A prompt that has not asked for bracketed paste is not typed into.
- **Enter once, and only on proof.** Enter is pressed a second after the paste, once it shows on the
  worker's screen: a row that changed carries this prompt's own number, or the
  screen holds one more of the program's long-paste placeholders (`[Pasted text …]`,
  `[Pasted Content …]`) than before, so an earlier prompt's leftover placeholder
  changing shape is not taken for this paste.
  With no proof in four seconds, or with a dialog, an error or the person's key in
  between, Enter is not pressed and the prompt fails, saying the words may be
  sitting at the worker's prompt. They are not cleared: the next prompt to that
  worker is pasted after them, and its Enter sends both.
- **A header line comes first**, naming the sender's pane and pid, saying a
  person drew the prompt line, and saying whether the worker's last message will
  be returned. It carries the prompt's number,
  `[Threshing Floor prompt <id>, from …]`.
- **One at a time.** One prompt in flight from a sender to a worker
  (`prompt_busy`), and a worker's prompts from several senders go in send order,
  each after the last has ended.
- **A retry is safe.** `key` names a prompt: the same key from the same pane
  returns the first prompt instead of typing another.
- **The pane's own write.** The bytes go through `TerminalView::type_prompt`, row
  #10 of `docs/security/terminal-input.md`, which refuses again for a pane a person
  has since closed to agents. Unlike the workbench's lines, a prompt reaches a
  worker in a tab that is not on screen: the header, not the person's view, is
  what says who typed it.

`prompt_status` reads it, by `prompt_id`, from the pane that sent it. Stages:
`waiting` (with what the worker is doing), `typing`, `delivered`, `working`,
`finished` and `failed` (with the reason). A prompt is called finished only after
the same settled rest. `finished` carries `reply`: the
worker's last message after the prompt, read from its own transcript (Claude
Code's or Codex's). The prompt's number in that transcript is what proves the
message answers this prompt; without it, `reply` is `null` and `reply_unknown`
says why. `wait_seconds` (up to 6) holds the answer until the prompt ends, so
one call can wait for the work.

An erased line, a pane marked read-only or off-limits, or MCP switched off stops a
prompt that has not been sent, including in the second between the paste and
Enter: the line is read again before Enter, and a line that is gone means no
Enter. A prompt already sent is the worker's and runs to its end. Every stage leaves a line in the same `asks.jsonl`, with `via` set
to `prompt`, the words' length and SHA-256, never the words and never the reply.

`floor_position` tells an agent where it stands: whether it is seated, its org
and role, the panes it reaches along a line and how far, and the panes that reach
it. That last list is the only place a worker learns who may prompt it before a
prompt arrives; a pane it cannot otherwise see is named there by pid, pane and tab
only, the numbers a prompt's own header carries. An agent Terminal Delight
launches is told all of this in its briefing (`surface::FLOOR_PARAGRAPH`): what a
line is, how to answer a prompt (one clear final message, because that is what
goes back), and how to hand work out (a work order with how it will be judged
done, prompt again when a reply falls short, and stop and tell the person after
two or three tries).

**What it does not stop.** An agent with a shell runs as you. It can edit TD's
state file, or put keystrokes into the window through the compositor. This is the
front door: the one TD offers, labels and answers, so that an agent which behaves
has a way to ask and a person has one place to say yes.

## Tools

### Git coordination

The GIT attention signal replaces the rotating engineering ticker. Click it for
projects in this window, expandable group previews, and the relationship evidence.
Arrow keys select, Enter opens, and Escape returns to the originating selection
before closing. Alt+G opens the inspector. Breadcrumbs navigate to their named
level; Back and Forward preserve navigation history. Ctrl+C copies an assessment
instruction and agent-visible context with the exact next read. Copying never
delivers a task, changes Git, or acknowledges a finding.

| Read tool | Arguments | Result |
|---|---|---|
| `coordination_overview` | `limit?`, `cursor?` | Urgent authorized projects, causes, coverage, observed groups and exact follow-up requests. |
| `coordination_project` | `project_id`, `checkout_id?`, `view?`, `limit?`, `cursor?` | Groups and checkout summaries; `view: "meanings"` pages the explanations. Specifying a checkout pages its file, rename and conflict evidence. |
| `coordination_cluster` | `project_id`, `cluster_id`, `limit?`, `cursor?` | Panes and known sessions joined to roots, branches, change sources, exact references and paged hotspots. |
| `coordination_changes` | `epoch`, `since_version` | Scope-local deltas, or `requires_full_snapshot: true` after a restart, permission change or expired history. |

Start with the overview and follow its `read` objects. Limits default to 40 and
accept 1–100. Cursors bind the epoch, presentation version, tool, project and selected
checkout or group. An expired cursor requires restarting that read. History keeps
32 presentation revisions; observation timestamps can refresh without creating a
revision. `fact_version` advances on evidence changes; `version` also advances on
attention changes. `attention_revision` names the metadata revision, and every
response declares its `attention_audience` and `catalog_version`.

Projects expose `meanings`: typed finding kinds, stable finding IDs, catalog keys,
typed parameters, headline, significance, limitations, evidence handles, material
revision, episode, condition and attributed attention state. The four catalog
kinds are shared working folders, shared paths between working copies, committed
target-check conflicts, and unresolved index entries. Observation gaps compose
with them. Unknown detector kinds retain evidence with an explicit fallback.
Conflicted path counts are affected files, never counts of logical conflicts.
A valid merge-check conflict remains a conflict even without per-file details.

Only the operator UI can mark a finding seen or declare sharing expected.
Expectations cover any panes in one named working folder, or an exact file and
working-copy set, on the recorded branches. Durations are 15 minutes, one hour
(default), or four hours. Controls show the scope before confirmation and allow
revocation. Branch or checkout identity changes invalidate the scope; a restart
requires reconfirmation. Expectations never cover conflicts or missing checks.
Acknowledged unresolved conflicts remain visible in the compact signal.

Metadata lives outside observed repositories in the user's TD state directory,
with owner-only permissions, atomic replacement, a nonblocking writer lock,
revision checks and request deduplication. Records expire from retained metadata
after seven days; storage is bounded to 512 records. Failed saves report failure
and retain observations. A corrupt store cannot quiet findings.

Fully exposed snapshots may carry `operator_attention`; that is the operator's
history, not an external coordinator's inbox. Restricted snapshots use
`independent_reader`, with no operator declarations or private metadata revision.
Read tools cannot write this metadata or impersonate its actor. External
coordinators keep their own cursors and acknowledgement policy.

Material signatures omit titles, routine measurement times, and changing commit
IDs. Exact evidence still advances `fact_version`. Failed checks retain prior
conditions as unverified; only a fresh comparable negative check establishes
absence. A path signature cannot detect every semantic change within that path.

Collection runs in the background. These tools use cached observations and execute
no Git commands. A shared checkout is counted once in file overlap; occupancy does
not establish authorship. Missing session identities remain unknown. Committed
overlap can include inherited changes from stacked branches. Target checks name
the measured HEAD and target object IDs; an unresolved index does not establish
which target caused it. Optional line analysis remains unmeasured.

Known-member path coverage and project membership coverage are separate. Missing
evidence can connect two observed groups, so a lower group count is not evidence
that friction improved. Source coverage qualifies every observation, including
empty file lists; unavailable measurements remain null or explicitly unavailable.

Authorization precedes grouping, identifiers and history. In a fully exposed
window, the native inspector and MCP use the same snapshot. A restricted scope
publishes permitted occupancy with file observations unavailable: Git cannot
attribute a filesystem change to the pane allowed to disclose it. Once a window
has held an off-limits pane, this conservative restriction remains for that
window's lifetime. The human operator can still inspect the broader observation,
but **Copy assessment context** uses the authorized projection.

The legacy `engineering_state` response retains its fields and requires a fully
exposed window. In a restricted window it refuses the read and points to
`coordination_overview`, preventing the old aggregate from bypassing exposure.

### Pane reads and appearance

0.4.0 registers seventeen tools: the four `coordination_*` reads above,
`engineering_state`, and the twelve below without a *(0.5.0 dev)* mark.
**0.5.0 development** adds five (`ask_agent`, `ask_answer`, `prompt_agent`,
`prompt_status`, `floor_position`), twenty-two in all.

In 0.4.0, `tools/list` declares `annotations.readOnlyHint: true` on the four
`coordination_*` tools only. **0.5.0 development:** it declares it on the
fourteen tools whose handlers only read: `list_panes`, `floor_position`,
`pane_events`, `get_pane_config`, `ask_answer`, `prompt_status`, the four
`coordination_*` tools, `engineering_state`, `document_notes`,
`surface_catalogue` and `grep`. Write tools leave this hint undeclared. These
hints help clients describe tools; they do not grant access or replace the
server's permission checks. The other risk hints remain undeclared.

When a listed pane has a sticky note, `note` carries `title`, `text` and
`pinned`. **0.5.0 development:** `note.by` is `"person"` or `"agent"` when the
writer is known. Older notes omit it because their writer is unknown. Keyboard
edits make the resulting note the person's; `leave_note` and the duplicate-tab
notice mark theirs as agent-written. The agent mark draws a robot stamp.
Restoring or copying a note preserves its recorded writer. Document annotations
are a separate feature; `ctl doc note` does not write sticky notes.

| Tool | Arguments | Returns |
|------|-----------|---------|
| `list_panes` | _(none)_ | Every currently-exposed pane: `tab`, `title`, `mode` (CLAUDE/CODEX/SHELL/REMOTE/`UNKNOWN`/the program's own name), `is_agent`, `pid`, `pane_id` (`null` for a window with no session host), `cwd`, `session` (resume recipe, which may not identify an exact conversation), `tool` (the tool in flight), `note`, and `exposed`. **`UNKNOWN` is a real answer, not a failure**: the pane belongs to a session host that has not classified it yet, and it is deliberately not reported as `SHELL` — an agent reading its own fleet can tell "nothing has described this pane" from "a shell is running here", which is the distinction whose absence hid eleven working agents from the attention rail (#462). **0.5.0 development:** `exposed` gives way to `reach` (how far the caller reaches this pane: `seen`, `read`, `ask` or `prompt`; a pane nobody seated in the Threshing Floor is `read`, and a seated pane appears only along a line a person drew from the caller's seat), and beside the panes `hidden` counts the panes the caller cannot see. Text summary + `structuredContent`. |
| `pane_events` | `pid` (from `list_panes`), `limit` (default 20, max 200) | Recent structured tool calls for that agent pane — `{ts, tool, summary}` — tailed from the agent's **own transcript**, not the rendered screen. Needs Events on. |
| `get_pane_config` | `targets?` — array of pids and/or `"outer"`; omit for every exposed pane + `outer` | Each target's **appearance**: the monitor grade as uniform `0..100` percents (`brightness`, `contrast`, `colour`, `text`, `background`, `gamma`, `menu_bar`, `text_size`, `bench_size`, `warp`, `crawl_angle`, `crawl_depth`, `phosphor`) plus `crawl` and `crt` booleans. A bad pid is a per-target `error` row, not a failed call. Read-only. **0.5.0 development:** also `field_colour`, `image_glow`, `inactive_image_glow`, `inactive_text`, `inactive_brightness`, `inactive_blur` and four linger times in milliseconds (`cursor_linger_ms`, `inactive_cursor_linger_ms` 0..220; `off_cursor_linger_ms`, `inactive_off_cursor_linger_ms` 0..650); optional lighting fields read `null` while unset. |
| `set_pane_config` | `updates` — array of `{ target, config }` where `target` is a pid or `"outer"` and `config` is a partial grade | Applies each partial: only the channels you include change (others untouched, PATCH semantics); out-of-range values clamp; an unknown channel is an error. A pane update may also carry `logo`, an absolute path to a png/jpg/jpeg/svg/webp image for the pane's agent-wall card (`""` clears it). Returns the resulting grade per target. Requires the **Writes** toggle. Setting `outer` re-grades every pane that inherits it — change the whole wall in one call. **0.5.0 development:** legacy `afterglow_ms` (0..1200) still works and resets the four linger overrides. |
| `leave_note` | `pid?`, `title?` (under 40 characters), `text` (ten words or fewer), `pin?`, `clear?` | Posts, replaces or peels the sticky note on a pane's glass, the same paper a person's Alt+S note uses. Omitting `pid` means the caller's own pane. Requires Writes. |
| `declare_deliverable` | `pid?`, `label?` (48 characters; defaults to the target's file name), `href` (absolute path or full URL), `clear?` | Names the one artifact this turn produced, as a click on the pane's attention-rail row and in its handed-over list. Relative paths, `javascript:` and `data:` are refused. Requires Writes. |
| `present_surface` | `pid`, `surface` (one TDSP document) | Puts a work object on that pane's workbench; the same `id` updates it, op `retire` removes it. Requires Writes. |
| `surface_catalogue` | _(none)_ | The kinds, actions and weights this build can render. Answers with the server off. |
| `document_notes` | _(none)_ | The notes map of the document beside the caller: a square floating over its pane, else a document pane it opened in its tab, else a document pane in its tab. Never another tab's. In 0.4.0 the caller's own pane must be exposed. |
| `open_document` | `path` (absolute or `file://`), `placement?` (`here`, the default, or `beside`) | Opens Markdown, HTML, an image, a video or a PDF over the caller's own pane, or in a pane beside it (at four panes, over it). Never in another tab; takes no pid. In 0.4.0 the caller's own pane must be exposed. Requires Writes. **0.5.0 development:** a square the person already has open over the pane is never replaced; the answer begins `held`, and the file is kept as the pane's handover on the artifact rail. |
| `engineering_state` | _(none)_ | Every project's git state as the rail read it, landing list included. A null field was not measured. Requires a fully exposed window; see *Git coordination* above. |
| `request_access` | `change` (`open_pane`, `close_pane`, `writes_on`, `expose_shells`, `events_on`; **0.5.0 development:** `ask_line`), `pid` (for `open_pane`, `close_pane` and `ask_line`), `why` (one sentence, at most 40 words) | Puts an ask on the person's attention rail with Yes and No, on the calling pane's own row. Changes nothing itself. Needs the master switch and a pane to call from; does **not** need Writes. See *An agent asks, a person answers* above. |
| `ask_agent` *(0.5.0 dev)* | `pid`, `question` (up to 4,000 characters), `copy?` | Asks a pane along a line that reaches `ask`. Without `copy`: the cheap answer, at once (`seat`, `summary`, `note`, `recent_tools`, `ask_id`, `via`). With `copy`: an `ask_id`, the `stage`, and what the copy reads (`reads`, `null` when unknown) against the `press_limit`. Refused by name without a line. Does not need Writes. See *Asking another agent* above. |
| `ask_answer` *(0.5.0 dev)* | `ask_id` | How a copy is getting on, and its answer once it has one. Only the pane that asked can read it. |
| `prompt_agent` *(0.5.0 dev)* | `pid`, `prompt` (up to 32,000 characters), `key?` (up to 64 characters) | Gives a pane work along a line that reaches `prompt`: the words are typed into its conversation when it is at rest. Returns a `prompt_id` and the stage at once. Refused by name without a prompt line. Does not need Writes: the prompt line is the person's grant. See *Prompting another agent* above. |
| `prompt_status` *(0.5.0 dev)* | `prompt_id`, `wait_seconds?` (0 to 6) | How a prompt is getting on (`waiting`, `typing`, `delivered`, `working`, `finished`, `failed`, `expired`), and the worker's last message once it has finished. Only the pane that sent it can read it. |
| `floor_position` *(0.5.0 dev)* | _(none)_ | Where the caller stands on the Threshing Floor: `seated`, `org`, `role` (`orchestrator`, `manager`, `worker`, `alone`), `lines_out` and `lines_in`, each `{pid, pane, tab, agent, rung, listed}`, where `listed` says whether that pane is in the caller's `list_panes` and `agent` is `null` for a pane that is not. `role` is read from those two lists, whole-org lines and bypasses included. Refused for a caller the window cannot place. |
| `grep` | `query` (exact, case-insensitive substring), `scrollback?` (recent lines/pane, default 2000, max 50000) | Searches every **exposed** pane's recent scrollback. Returns, per matching pane, its identity (`pid`/`tab`/`title`/`mode`) and the matching lines as `{line, col, text}` (capped 50/pane). Read-only — it reads on-screen text, never writes. Use it to find *where* something is across the whole wall (an error, a path, a value). |

Every tool but `surface_catalogue` honours the master switch; `pane_events` also
needs Events on and an agent pane; the writing tools also need the Writes toggle.
`grep` reads on-screen content of exposed panes, so the expose policy governs
that disclosure exactly as it gates `list_panes`. Where a tool takes an optional
`pid`, leaving it out means the caller's own pane, which only a relay that named
its caller can resolve.

### The config API is deliberately *dumb*

`set_pane_config` stores the **absolute** number you give it — it never
interprets a relative ask. To "make every terminal 20% dimmer": `get_pane_config`
the current `brightness`, compute `brightness * 0.8` yourself, then
`set_pane_config` that value. The agent owns the arithmetic; the surface just
reads and writes. Every channel speaks the same `0..100` percent scale (it is the
GAUGES slider position) so you never reason about a channel's internal range;
**0.5.0 development:** the linger fields are the exception, in milliseconds. Appearance changes persist exactly like an OSD edit and apply on the gpui
main thread (bounded by the same 5 s budget as reads — a wedged UI errors, never
hangs).

## Push notifications

Over the in-process `TD_MCP` transport only (the relay carries no pushes),
after `initialize`, with the server and Events on and the client's log level at
`info` or below (the default), the server pushes `notifications/message` about
once a second as the wall changes, so you can react instead of polling:

```jsonc
{ "jsonrpc": "2.0", "method": "notifications/message",
  "params": { "level": "info", "logger": "terminal-delight",
    "data": { "event": "tool_call", "pid": 4242, "title": "build",
              "tool": "Bash", "summary": "cargo test", "ts": "2026-…" } } }
```

`data.event` is one of:

- `agent_appeared` — a new exposed agent pane (carries `pid`, `title`, `mode`, `cwd`, `session`).
- `tool_call` — an agent called a tool (carries `pid`, `title`, `tool`, `summary`, `ts`).
- `agent_vanished` — a watched pane is gone (carries `pid`).

Each event names the fields that quote a pane (`title`, `cwd`, `tool`,
`summary`) under `data.untrusted_terminal_data`.

**0.5.0 development:** the feed reaches every connection at once and cannot tell
who is listening, so it carries only panes nobody seated in the Threshing Floor;
a seated pane's tool calls are never pushed.

The feed never replays history: when you connect mid-conversation you get one
`agent_appeared` per current agent, then only events that happen **after** you
start watching. Raise the log level (`logging/setLevel` to `warning`+) to
silence it.

## Security posture

- **No way into a terminal in 0.4.0.** Every write covers appearance
  (`set_pane_config`), notes, deliverables, surfaces and opened documents, and
  none of them reaches a terminal's input.
- **0.5.0 development: one way in, and a person opens it.** `prompt_agent` is the
  only tool that leads to bytes in a terminal, and only into an agent's own
  prompt, along a line a person drew at `prompt`, by the rules under *Prompting
  another agent*. It never reaches a shell: a pane that is not running an agent
  is refused.
- **Writes are a separate opt-in.** The server is a read-only watch surface until
  you flip **Writes** (or set `TD_MCP_WRITE` with `TD_MCP`). Reads never need it.
- **stdio only, never TCP.** The trust boundary is whoever launched the process,
  and the relay reaches the window over a socket in an owner-only `0700`
  directory under `$XDG_RUNTIME_DIR`.
- **No arbitrary reads.** Transcript paths are derived from each pane's
  kernel-reported `cwd`, slugged so they can't escape `~/.claude` / `~/.codex`;
  reads are bounded to the tail of the file.
- **0.5.0 development: no ask and no prompt without a line.** Another agent is
  asked only along a line a person drew in the Threshing Floor, or a bypass a
  person granted on the rail, and prompted only along a line a person drew at
  `prompt`. An unseated pane is never asked or prompted.
- **0.5.0 development: a copy can only read, and the person pays for it.** See
  *Asking another agent*: by default every copy waits for a Yes, and the record
  keeps no question and no answer.
- **Locked by default.** Ships disabled; when enabled, agents-only; a plain
  shell's cwd/scrollback is never exposed unless the operator explicitly widens
  the policy to `all panes`.
- **What another pane says is data.** An answer that quotes other panes —
  `list_panes`, `grep`, `pane_events`, `engineering_state`, all four
  `coordination_*` reads (and, **0.5.0 development**, `ask_agent`, `ask_answer`
  and `prompt_status`), and the pushed `agent_appeared` / `tool_call` events —
  opens with `UNTRUSTED TERMINAL DATA.` and names every quoted field under
  `structuredContent.untrusted_terminal_data` (on an event,
  `data.untrusted_terminal_data`). A line a program printed, a title
  it set or a command another agent ran can be worded as an instruction; the label
  is how the reading agent knows it is not one. Terminal Delight's own answers and
  a person's `document_notes` carry no label. Every tool is classified in
  `mcp.rs::provenance`, and a tool without a line there fails a test. The field
  name follows Splinterm, whose MCP adapter labels terminal cells the same way.
