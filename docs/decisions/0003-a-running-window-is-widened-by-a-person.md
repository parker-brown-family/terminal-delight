# 0003 — A running window is widened by a person

Date: 2026-10-21
Status: accepted
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
them. Since pull request 957 an agent that
wants one of them turned on asks with `request_access`, the ask waits on the
attention rail, and only a person's Yes answers it. A test keeps the MCP server
and the control socket from naming the functions that answer an ask.

The control socket did not need them. `terminal-delight ctl mcp writes on`,
`mcp on` and `mcp expose all` set the switches directly, and any process running
as you can reach the socket — the agent the rail had just told to wait included.
The answer to the question above was yes, in one shell command.

## What changed

**The socket narrows the switches and refuses to widen them.** `mcp off`, `mcp
writes off` and `mcp expose agents` apply at once, as they always did: giving
up access never needs permission. The three that widen are answered with one
`err` line that changes nothing and says what to do instead — the person turns
it on in the MCP control panel (Ctrl+Shift+A), and an agent asks them through
`request_access`.

**A window launched with `TD_MCP_SCRIPTED=1` widens as before.** The test rigs
under `scripts/` launch a window of their own, on a session of their own, and
drive it over the socket with nobody at it; they now set the variable where they
launch. Exactly `1` opts in.

## Why the launch, and not the socket

Decision 0002 still holds: every process that can reach the socket is the same
user, and a peer-credential or pane-ancestry check would distinguish between
processes that are all equally you. Nothing on the socket can tell a person's
script from an agent's shell command, so nothing on the socket can carry the
authority to widen.

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

Two neighbours stay as 0002 left them. The bench commands act on an agent
pane's bench without asking anybody: `ctl bench say` sends a line through its
composer, `ctl bench choose` and `ctl bench submit` press its answers, and
`ctl bench click` presses whatever is under the point, pause and end agent
included. A press reaches the agent as keys into a live picker or as a
`[workbench]` line, only while that pane is on screen, and flashes when it
does. All of them but `bench click` refuse a pane marked off-limits; that gap is
issue 1090 — ctl bench click can press an off-limits pane's bench. And the pane
a caller names in `mcp from` is still taken on its word.

## When this expires

With 0002, and for the same reason: the first caller able to reach a window from
outside this user's runtime directory — a network listener, a socket another
user can open, a remote or multiplayer access point. At that moment the launch
environment stops being the only thing a caller cannot reach, and widening has
to be granted to a proven identity rather than to whoever started the process.
