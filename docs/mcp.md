# terminal-delight as an MCP control surface

terminal-delight can act as an **[MCP](https://modelcontextprotocol.io)
server**: an orchestrating agent connects over stdio and watches a wall of
panes — who is running where, which conversation each agent is in, and what
tools they call — and reacts, **without ever touching a keyboard**. It can also,
behind a second opt-in, **configure a pane's appearance** — dim the brightness,
flatten the warp, restyle the whole wall at once.

The MCP tools in this build do not offer arbitrary shell input. "Writes" permit
UI changes: appearance, notes, work objects and documents. They are disabled
until you turn them on. Standard processes retain your account's authority:
the separate control socket has scripted bench-input and command-launch routes,
and the session host accepts terminal input from same-user clients. MCP switches
and off-limits marks alone do not contain hostile Standard processes.

## Wiring it up

Connect to the window already running with the stdio relay:

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

The relay carries JSON-RPC to the running window's control socket. The four Git
coordination reads use this existing transport. Every reply names its instance
(session, window PID and build); check that it identifies the window you intend
to coordinate. A request that cannot reach a ready window returns an error.

A display is needed for live observations, because the window owns the pane
snapshot. `initialize`, `tools/list` and `ping` are snapshot-independent.
`tools/call` has a bounded wait and returns `-32000 "terminal-delight UI not ready"`
when the window cannot answer.

A separately launched GUI can also serve MCP directly over piped stdin/stdout
with `TD_MCP=1`. That advanced mode reports its own window; it is not required to
attach an agent to an existing Terminal Delight session.

## Turning exposure on

Exposure ships **disabled** and the policy is operator-controlled from the
**MCP CONTROL** panel (the robot button on the mother bar):

- **Enabled** — master switch. Off ⇒ the server reports nothing.
- **Expose** — `agents only` (claude / codex panes; the safe default, never a
  plain shell) or `all panes`.
- **Events** — allow tailing each agent's transcript for tool-call events
  (powers `pane_events` and the push feed).
- **Writes** — a *second* opt-in (off by default) that promotes the read-only
  watch surface to a remote-control one: with it on, `set_pane_config` may change
  a pane's (or the window-level `outer`) appearance, and agents may leave notes,
  declare deliverables, present surfaces and open documents. None of it reaches a
  terminal's input. A read→write escalation, so
  it's its own switch. The `TD_MCP_WRITE=1` env var forces it on for a headless /
  orchestrated launch.

The policy is persisted in the session layout under `[mcp]`. A connected client always
sees the *current* policy live; flipping a toggle takes effect on the next call.

### One pane, off-limits

Those switches cover the whole window. To say *not this one*, right-click inside
a pane and choose **Off-limits to agents**: the pane wears `OFF-LIMITS` in its
header, and no agent reads it or writes to it under any policy — it is gone from
`list_panes`, `grep`, `pane_events`, the pushed events and the appearance tools,
and a note, deliverable or surface aimed at it from another pane is refused. The
agent running *inside* it keeps its own-pane verbs: it can still present its own
work, declare its own deliverable and post its own note, because the mark hides
a pane from other agents rather than silencing the one inside it. Own-document open/read also remain available. The ctl connection obtains the peer PID through `SO_PEERCRED`; its claimed pane survives only when the current pane shell is that process or its ancestor. Forged or unavailable ancestry removes own-pane authority. The same menu
row lifts it (**Let agents see this pane**). It is saved with the layout.

Only a person sets or lifts it. There is no `ctl` verb and no MCP verb for it.
The control socket's bench-input verbs, synthetic click/hover, probe and scripted
face selection refuse off-limits panes. This is enforced on selection and again
at synthetic pointer dispatch. Document ctl verbs and tab edits also refuse hidden targets; queued bench input waits while its pane is off-limits. Hidden and absent write targets answer alike. The MCP gate is
`mcp::should_expose(cfg, is_agent, off_limits)` and its write half
`mcp::may_write`; the idea is Splinterm's per-pane grant, with TD's own shape.

### An offline command with less authority (0.5 development)

Standard terminals keep ordinary account access. For code you want confined,
explicitly start a Restricted command before its code or startup files run:

```sh
terminal-delight restricted run --workdir "$HOME/Projects/example" -- sh -c 'make test'
```

See the dedicated [Restricted commands page](../docsite/pages/restricted.html) for the complete user guide. The canonical selected path must be strictly below the OS account home, with no dot-directory in that path. Hidden files already inside a selected project remain granted content; system trees, hidden roots and external volumes need a dedicated project copy.

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

### An agent asks, a person answers

A refusal about access used to be the end of it. Now an agent can **ask**:
`request_access` puts its ask on the attention rail, in its own lane, **ACCESS**,
drawn in amber and sorted above every other lane (only a row a person has
promoted sits higher). The row says what is being asked in plain
words ("Let agents see “prod-db”"), quotes the agent's reason, and carries two
buttons, **Yes** and **No**; with the queue open, `y` and `n` answer the row under
the cursor.

There are five things to ask for, all of them about who agents can see and what
agents may do:

| `change` | Yes does | The person's own control for it |
|---|---|---|
| `open_pane` + `pid` | lifts off-limits from that pane | right-click → Let agents see this pane |
| `close_pane` + `pid` | marks that pane off-limits | right-click → Off-limits to agents |
| `writes_on` | turns MCP writes on | MCP panel, Writes |
| `expose_shells` | shows plain shells as well as agents | MCP panel, Expose |
| `events_on` | shows other agents' tool calls | MCP panel, Events |

Appearance is not on the list. With writes on, `set_pane_config` restyles a pane
and asks nobody — Parker's line was that agents should not be *"blocked by human
decision for simple things like themeing"*.

How it behaves:

- **Nothing changes until a person presses Yes.** No answers on a timer, no
  answer by default. An unanswered ask is a row waiting, like any other.
- **One ask per pane.** A second ask from the same pane replaces the first, so
  one agent cannot fill the rail, and an ask dies with its pane. Asks are not
  saved; a window restart drops them.
- **An ask for what is already so is refused** before it reaches the rail
  ("pane 4412 is not off-limits, so there is nothing to ask"), so a Yes always
  does something.
- **The answer reaches the agent as one signed line** typed into its own
  terminal: `[workbench:<tag>] request_access open_pane 4412: yes, pane 4412 is
  open to agents`. It goes through the bench's own path, so it is typed when that
  pane is on screen and held until then. The setting does not wait for the line:
  it changes the moment the person answers, so an agent that retries sees it.
- **It works with writes off**, because `writes_on` is one of the things an agent
  may need to ask for.

What can answer an ask is the rail's two buttons and its two keys, and nothing
else. No MCP verb carries an answer (the ask rides `ConfigPatch` as a
`serde(skip)` field, so no agent JSON can put one in either), and no `ctl` verb
reaches the rail: `ctl bench click|hover` drives a pane's bench through that
pane's own handlers. `access::tests::no_agent_facing_surface_can_answer_an_ask`
keeps `mcp.rs` and `ctl.rs` from naming the answering code, and
`an_ask_is_answered_only_by_the_rails_buttons_and_keys` keeps the rest of the
window to those two callers.

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
but **Copy agent-visible context** uses the authorized projection.

The legacy `engineering_state` response retains its fields and requires a fully
exposed window. In a restricted window it refuses the read and points to
`coordination_overview`, preventing the old aggregate from bypassing exposure.

### Pane reads and appearance

| Tool | Arguments | Returns |
|------|-----------|---------|
| `list_panes` | _(none)_ | Every currently-exposed pane: `tab`, `title`, `mode` (CLAUDE/CODEX/SHELL/REMOTE/`UNKNOWN`/the program's own name), `is_agent`, `pid`, `cwd`, `session` (resume recipe, which may not identify an exact conversation), `exposed`. **`UNKNOWN` is a real answer, not a failure**: the pane belongs to a session host that has not classified it yet, and it is deliberately not reported as `SHELL` — an agent reading its own fleet can tell "nothing has described this pane" from "a shell is running here", which is the distinction whose absence hid eleven working agents from the attention rail (#462). Text summary + `structuredContent`. |
| `pane_events` | `pid` (from `list_panes`), `limit` (default 20, max 200) | Recent structured tool calls for that agent pane — `{ts, tool, summary}` — tailed from the agent's **own transcript**, not the rendered screen. |
| `get_pane_config` | `targets?` — array of pids and/or `"outer"`; omit for every exposed pane + `outer` | Each target's **appearance**: the monitor grade as uniform `0..100` percents (`brightness`, `contrast`, `colour`, `text`, `background`, `gamma`, `menu_bar`, `text_size`, `warp`, `crawl_angle`, `crawl_depth`) plus a `crawl` boolean. A bad pid is a per-target `error` row, not a failed call. Read-only. |
| `set_pane_config` | `updates` — array of `{ target, config }` where `target` is a pid or `"outer"` and `config` is a partial grade | Applies each partial: only the channels you include change (others untouched, PATCH semantics); out-of-range values clamp. Returns the resulting grade per target. Requires the **Writes** toggle. Setting `outer` re-grades every pane that inherits it — change the whole wall in one call. |
| `request_access` | `change` (`open_pane`, `close_pane`, `writes_on`, `expose_shells`, `events_on`), `pid` (for the two pane changes only), `why` (one sentence, at most 40 words) | Puts an ask on the person's attention rail with Yes and No, on the calling pane's own row. Changes nothing itself. Needs the master switch and a pane to call from; does **not** need Writes. See *An agent asks, a person answers* above. |
| `grep` | `query` (exact, case-insensitive substring), `scrollback?` (recent lines/pane, default 2000, max 50000) | Searches every **exposed** pane's recent scrollback. Returns, per matching pane, its identity (`pid`/`tab`/`title`/`mode`) and the matching lines as `{line, col, text}` (capped 50/pane). Read-only — it reads on-screen text, never writes. Use it to find *where* something is across the whole wall (an error, a path, a value). |

`list_panes`/`pane_events`/`get_pane_config`/`grep` honour the master switch;
`pane_events` also needs Events on and an agent pane; `set_pane_config` also
needs the Writes toggle. `grep` reads on-screen content of exposed panes, so the
expose policy governs that disclosure exactly as it gates `list_panes`.

### The config API is deliberately *dumb*

`set_pane_config` stores the **absolute** number you give it — it never
interprets a relative ask. To "make every terminal 20% dimmer": `get_pane_config`
the current `brightness`, compute `brightness * 0.8` yourself, then
`set_pane_config` that value. The agent owns the arithmetic; the surface just
reads and writes. Every channel speaks the same `0..100` percent scale (it is the
display-tray slider position) so you never reason about a channel's internal
range. Appearance changes persist exactly like an OSD edit and apply on the gpui
main thread (bounded by the same 5 s budget as reads — a wedged UI errors, never
hangs).

## Push notifications

With the client's log level at `info` or below (the default), the server pushes
`notifications/message` as the wall changes, so you can react instead of
polling:

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

The feed never replays history: when you connect mid-conversation you get one
`agent_appeared` per current agent, then only events that happen **after** you
start watching. Raise the log level (`logging/setLevel` to `warning`+) to
silence it.

## Security posture

- **Never a PTY.** No tool writes bytes to a terminal. Sending input into a shell
  would be arbitrary code execution; that path does not exist here. Writes cover
  appearance (`set_pane_config`), notes, deliverables, surfaces and opened
  documents, and none of them reaches a terminal's input.
- **Writes are a separate opt-in.** The server is a read-only watch surface until
  you flip **Writes** (or set `TD_MCP_WRITE`). Reads never need it.
- **stdio only, never TCP.** The trust boundary is whoever launched the process.
- **No arbitrary reads.** Transcript paths are derived from each pane's
  kernel-reported `cwd`, slugged so they can't escape `~/.claude` / `~/.codex`;
  reads are bounded to the tail of the file.
- **Locked by default.** Ships disabled; when enabled, agents-only; a plain
  shell's cwd/scrollback is never exposed unless the operator explicitly widens
  the policy to `all panes`.
- **What another pane says is data.** An answer that quotes other panes —
  `list_panes`, `grep`, `pane_events`, `engineering_state`, all four `coordination_*` reads, and the pushed
  `agent_appeared` / `tool_call` events — opens with `UNTRUSTED TERMINAL DATA.`
  and names every quoted field under `structuredContent.untrusted_terminal_data`
  (on an event, `data.untrusted_terminal_data`). A line a program printed, a title
  it set or a command another agent ran can be worded as an instruction; the label
  is how the reading agent knows it is not one. Terminal Delight's own answers and
  a person's `document_notes` carry no label. Every tool is classified in
  `mcp.rs::provenance`, and a tool without a line there fails a test. The field
  name follows Splinterm, whose MCP adapter labels terminal cells the same way.

Publication is an authorized output channel, not a semantic safety guarantee.
A note, terminal output or shared file can mislead an unrestricted reader that
chooses to act on it. The profile has no automatic handoff operation; it cannot
constrain the later actions of a Standard program reading its authorized output.
Secrets already inside the chosen directory are granted content too.
