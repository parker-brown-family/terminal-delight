# terminal-delight as an MCP control surface

terminal-delight can act as an **[MCP](https://modelcontextprotocol.io)
server**: an orchestrating agent connects over stdio and watches a wall of
panes — who is running where, which conversation each agent is in, and what
tools they call — and reacts, **without ever touching a keyboard**. It can also,
behind a second opt-in, **configure a pane's appearance** — dim the brightness,
flatten the warp, restyle the whole wall at once.

The hard line never moves: **no tool can write bytes to a terminal/PTY.**
Sending input into a shell would be arbitrary code execution; that path does not
exist. "Writes" here mean *appearance only* (the monitor grade), and they are
disabled until you deliberately turn them on.

## Wiring it up

terminal-delight speaks MCP on stdin/stdout when launched with the `TD_MCP`
environment variable set. Point your MCP client at the binary:

```jsonc
{
  "mcpServers": {
    "terminal-delight": {
      "command": "/path/to/terminal-delight",
      "env": { "TD_MCP": "1" }
    }
  }
}
```

When the client launches the server, terminal-delight opens its window **and**
serves JSON-RPC on the piped stdio. That window *is* the wall the server
reports on.

### Two things to understand before you rely on it

1. **The server sees the panes of the instance that is serving — its own.**
   The MCP server reports the panes of *the terminal-delight process the client
   launched*, not some other already-running copy. The intended model is
   therefore **the orchestrator owns the wall**: it launches terminal-delight
   (with no other instance running), the saved session restores its agent panes
   (`claude --resume …` / `codex resume …`), and the server watches them. If a
   terminal-delight is *already* running when the client launches another with
   `TD_MCP=1`, the second process boots as a small scratch window and will only
   report that — "attach to an already-running instance" is a future increment.

2. **It needs a display.** terminal-delight is a GUI; the MCP server is a
   companion to a visible wall. The JSON-RPC **handshake** (`initialize`,
   `tools/list`, `ping`) works regardless, but `tools/call` needs the window to
   be up to read live panes — on a headless host with no display, those calls
   return a `-32000 "terminal-delight UI not ready"` error instead of hanging.

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
  a pane's (or the window-level `outer`) appearance. A read→write escalation, so
  it's its own switch. The `TD_MCP_WRITE=1` env var forces it on for a headless /
  orchestrated launch.

The policy is persisted in `state.toml` under `[mcp]`. A connected client always
sees the *current* policy live; flipping a toggle takes effect on the next call.

## Tools

| Tool | Arguments | Returns |
|------|-----------|---------|
| `list_panes` | _(none)_ | Every currently-exposed pane: `tab`, `title`, `mode` (CLAUDE/CODEX/SHELL/…), `is_agent`, `pid`, `cwd`, `session` (resumable id), `exposed`. Text summary + `structuredContent`. |
| `pane_events` | `pid` (from `list_panes`), `limit` (default 20, max 200) | Recent structured tool calls for that agent pane — `{ts, tool, summary}` — tailed from the agent's **own transcript**, not the rendered screen. |
| `get_pane_config` | `targets?` — array of pids and/or `"outer"`; omit for every exposed pane + `outer` | Each target's **appearance**: the monitor grade as uniform `0..100` percents (`brightness`, `contrast`, `colour`, `text`, `background`, `gamma`, `menu_bar`, `text_size`, `warp`, `crawl_angle`, `crawl_depth`) plus a `crawl` boolean. A bad pid is a per-target `error` row, not a failed call. Read-only. |
| `set_pane_config` | `updates` — array of `{ target, config }` where `target` is a pid or `"outer"` and `config` is a partial grade | Applies each partial: only the channels you include change (others untouched, PATCH semantics); out-of-range values clamp. Returns the resulting grade per target. Requires the **Writes** toggle. Setting `outer` re-grades every pane that inherits it — change the whole wall in one call. |

`list_panes`/`pane_events`/`get_pane_config` honour the master switch;
`pane_events` also needs Events on and an agent pane; `set_pane_config` also
needs the Writes toggle.

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
  would be arbitrary code execution; that path does not exist here. The only
  writes are *appearance* (the monitor grade) via `set_pane_config`.
- **Writes are a separate opt-in.** The server is a read-only watch surface until
  you flip **Writes** (or set `TD_MCP_WRITE`). Reads never need it.
- **stdio only, never TCP.** The trust boundary is whoever launched the process.
- **No arbitrary reads.** Transcript paths are derived from each pane's
  kernel-reported `cwd`, slugged so they can't escape `~/.claude` / `~/.codex`;
  reads are bounded to the tail of the file.
- **Locked by default.** Ships disabled; when enabled, agents-only; a plain
  shell's cwd/scrollback is never exposed unless the operator explicitly widens
  the policy to `all panes`.
