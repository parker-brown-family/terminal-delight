# 0003 — A running window's MCP policy is changed by a person

Date: 2026-10-21
Status: accepted; amended the same day, when narrowing was refused too (the file keeps
its first name, "a running window is widened by a person")
Evidence: the control socket's `mcp writes on` answering `ok` from inside a
pane on 2026-10-21; `access.rs`, which made agents ask for access on the
attention rail (pull requests 956 and 957)
Arises from: a question asked in public — *"does the mcp control socket
authenticate clients, or is it local-socket trust only? curious whether a rogue
process in a pane could reach the relay and flip the write-exposure toggle"*

Three of the MCP switches decide what agents may do: the server itself, writes,
and whether plain shells are shown to them. Writes are UI changes: with them an
agent can restyle panes, leave notes on them, and present or open documents,
but not type into a terminal. If writes ever gate typing into another agent, as
the native agent-to-agent handoff branch proposes, the stakes here change with
them. Since pull request 957 an agent that wants writes or plain shells turned on
asks with `request_access`, the ask waits on the attention rail, and only a
person's Yes answers it. The server itself can only be turned on in the panel:
`request_access` needs it running. A test keeps the MCP server
and the control socket from naming the functions that answer an ask.

The control socket did not need them. `terminal-delight ctl mcp writes on`,
`mcp on` and `mcp expose all` set the switches directly, and any process running
as you can reach the socket — the agent the rail had just told to wait included.
The answer to the question above was yes, in one shell command.

## What changed

The initial patch refused widening and permitted reductions. The complete 0.5 design supersedes that choice: an unscripted socket refuses all six global mutations. Disabling other workloads' access is also an exercise of authority, not a caller merely giving up its own grant. The person changes policy in the MCP panel; existing `request_access` remains a proposal route for wider MCP access.

A window explicitly launched with `TD_MCP_SCRIPTED=1` retains all six mutations for operator automation. Exactly `1` opts in. This is cooperative Standard automation, not a hostile-host boundary.

The optional offline Restricted command profile places workload code and children behind OS-enforced namespaces with one work directory and a narrow per-run gateway. It cannot reach the host control socket, saved state, desktop automation or native endpoints. Its initial TD grant is only an own-pane text note; automatic handoffs and online providers are unavailable. See [the selected architecture](../plans/process-authority/02-architecture.md).

## Why the launch, and not the socket

Decision 0002 still holds: every process that can reach the socket is the same
user, and a peer-credential or pane-ancestry check would distinguish between
processes that are all equally you. Nothing on the socket can tell a person's
script from an agent's shell command, so nothing on the socket can carry the
authority to change global policy, in either direction.

The launch can. A process inside a pane cannot change the environment of a
window that is already running, so `TD_MCP_SCRIPTED` is read once, when the
socket starts, and nothing that runs afterwards can change the answer. This is
the same door `TD_MCP_WRITE` has always been: whoever launches a window chooses
what it starts with.

The socket does not raise an ask on the rail itself. An ask needs a reason a
person can judge and a pane to type the answer into; `request_access` carries
both, and a shell can reach it through `ctl mcp from <session> <pane> rpc`.

## What this does not close

A process running as you can still widen at the **next launch**:

- **Launch a second window on the same session** with `TD_MCP_SCRIPTED=1`, or
  with `TD_MCP=1 TD_MCP_WRITE=1`. It inherits `TD_SESSION` from its pane,
  attaches to the running host and takes each pane's stream. The new window
  appears, and the first window's panes stop updating: they keep what they last
  drew, and the reason goes only to the log.
- **Edit the session file** the window loads, then wait for a restart or cause
  one.

Neither changes a window that is already running; each takes effect only in a
window launched afterwards. Neither can be closed from inside the program while
every caller is the same user, because the file and the launch are the user's
own.

Two neighbours stay much as 0002 left them. The bench commands act on an agent
pane's bench without asking anybody, except on a pane marked off-limits: `ctl bench say` sends a line through its
composer, `ctl bench choose` and `ctl bench submit` press its answers, and
`ctl bench click` presses whatever is under the point, pause and end agent
included. A press reaches the agent as keys into a live picker or as a
`[workbench]` line, only while that pane is on screen, and flashes when it
does. The 0.5 change also refuses off-limits panes for synthetic pointer, probe and scripted face selection, closing issue 1090. The session in `mcp from` prevents misrouting. The control socket captures its kernel peer PID, and the current pane snapshot retains a claimed pane only for descendants of that pane's shell. A forged claim is downgraded to no pane; the Restricted supervisor and a genuine relay retain their own-pane authority. This is process attribution within the account, not a defence against hostile Standard host processes.

## When this expires

With 0002, and for the same reason: the first caller able to reach a window from
outside this user's runtime directory — a network listener, a socket another
user can open, a remote or multiplayer access point. At that moment the launch
environment stops being the only thing a caller cannot reach, and widening has
to be granted to a proven identity rather than to whoever started the process.
