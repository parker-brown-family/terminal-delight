# What can put bytes into a terminal

Terminal Delight owns a pty per pane. Anything that writes to that pty is typing into your shell,
and in an agent pane it is typing into a live conversation with a model that will act on it.

**The rule this repository holds: TD never types into a running terminal on its own initiative.**
Every byte that reaches the pty of a pane you are already using originates from a key you pressed, a
pointer gesture you made (a scroll, a ▲/▼ click, a context-menu paste), or the terminal protocol
answering the program running in the pane. No timer, plugin, MCP verb or notification callback can
put text in front of an agent that is already running.

One qualification, stated here rather than buried, because the unqualified sentence is not true:
**opening a new pane can carry a command line into it** — the control socket's `adopt` verb, the
`TD_SEED_RESUME` launch variable, and session restore all land on site #8, and one shipped plugin
puts that verb in front of agents. What none of them can do is reach a pane that already exists,
because none of the requests can name one. See *The one to keep an eye on*.

This page enumerates every write path so that claim is checkable rather than asserted.

---

## The enumeration

There is one primitive — `session.notifier.notify(bytes)` in `app/src/pane.rs`. Every call site,
audited 2026-09-08:

| # | Site | Enclosing function | What initiates it | Class |
|---|---|---|---|---|
| 1 | `pane.rs` | `send` | `on_key` — **its only caller** | keystroke |
| 2 | `pane.rs` | `handle_term_event` → `TermEvent::PtyWrite` | the program in the pane asking the terminal a question (cursor reports, device attributes) | protocol reply |
| 3 | `pane.rs` | `render` | focus in/out, `\x1b[I` / `\x1b[O`, only when the program set `FOCUS_IN_OUT` | protocol reply |
| 4 | `pane.rs` | `scroll_by_wheel` ×2 | mouse wheel over an alt-screen program, translated to arrow keys | pointer |
| 5 | `pane.rs` | `cut_selection` | the cut keybinding — sends DEL for the removed characters | keystroke |
| 6 | `pane.rs` | `paste_clipboard` | the paste keybinding, and the right-click context menu's *Paste* row | keystroke + pointer |
| 7 | `pane.rs` | `seek_agent_prompt` | `alt+↑` / `alt+↓` **and the ▲/▼ header buttons** — walks an alt-screen agent's own scrollback with synthetic wheel notches, or PageUp/PageDown when the program has not asked for mouse reports; a bounded async walk, see below | keystroke + pointer |
| 8 | `pane.rs:2471` | `new_restored` | a **freshly spawned** shell is handed its recorded command line — session restore, the dead-agent *resurrect* menu, `ctl adopt --run`, or `TD_SEED_RESUME` at launch | see below |

`send` is the only site that composes bytes from a keystroke, and it is reachable only from the key
handler. That is the invariant. Everything else is the terminal answering its own program (#2, #3),
a pointer gesture the person made (#4, #6, #7), or site #8 handing a command line to a shell it just
spawned — and #8 is the only one of the three that something other than a person can start.

**`mcp.rs` and `plugins.rs` reach none of them.** The MCP tool surface changes appearance —
brightness, contrast, warp, text size — reads pane state, and posts a sticky note; the seven verbs
are `list_panes`, `pane_events`, `get_pane_config`, `set_pane_config`, `leave_note`, `grep` and
`ping`. There is no input verb, by design, and this audit confirmed it rather than taking the docs'
word for it. The plugin *host* launches plugin MCP servers over stdio and never holds a `Session`.

**`ctl.rs` reaches exactly one — site #8, and only by making a new pane.** `ctl adopt --cwd X --run
"<cmd>"` travels `AdoptReq` → `Workspace::queue_adopt` → `drain_pending_adopts` → `adopt_pane` →
`make_pane_restored` → `new_restored`, and the command line lands in a shell that did not exist a
moment earlier. It is what `scripts/td-send` uses to migrate a desktop tile into TD. The bound that
makes this acceptable is structural, not a promise: `adopt` has no pane argument, so there is no
expressible request that writes into a session already running. The socket itself lives in a
`0700` directory keyed to the owning pid, so the caller is already this user — and a process running
as this user can execute commands without asking a terminal emulator to do it.

**And a plugin this repository ships puts that verb in front of agents.** `plugins/td-send/`
declares `pull_workspace` and describes itself as "the ⇄ desk-consolidation verbs behind
SUPER+ALT+T, **exposed to agents**"; it shells out to `scripts/td-send`, which sends the `adopt`
line. So the honest statement is not "no plugin can cause TD to type". It is that the only thing any
caller can cause is a **new pane running a command**, because that is the only shape the socket can
express. An agent that calls `pull_workspace` gets a tile migrated into a fresh tab. It does not get
a byte into the pane it is living in, or into any other pane already running.

---

## What was removed, and why

**Cache keepalive.** It typed a message into an idle agent's prompt before the model's prompt cache
expired, then pressed Enter for you: 💤 on the tab at 50 minutes, the message staged at 55, a
desktop notification, auto-send at 57. It existed because a cold return re-writes the whole
conversation at 2.0× input instead of reading it at 0.1×, which measured at roughly $230 of
avoidable spend across thirty days on one machine.

It was gated behind `TD_KEEPALIVE` and off by default. It is still gone, and the gate is not a
defence: an open-source project is read as source, and a feature that synthesises keystrokes into a
live agent session is a finding whether or not a default enables it. A sibling plugin was flagged in
security review for the same shape.

The feature's own source recorded the failure mode plainly, having already hit it once:

> its runtime reported a pane as idle and ready to receive input while Claude Code was sitting on
> its "Do you trust the files in this folder?" dialog, because a status describes the agent PROCESS,
> not the screen. Enter there answers the dialog.

The mitigation that followed — look at the grid and confirm the typed text is still visible before
pressing Enter — narrowed the window without closing it. A heuristic that reads a screen cannot be
made safe enough to press Enter on somebody's behalf, and the correct amount of automated Enter in
a terminal emulator is none.

Removed with it, because neither has any meaning without it: the AWAY campfire (🔥/🌙 in the menu
bar, whose entire readout was *whether we are typing into your terminals*) and the 💤 drowsy tab
badge.

---

## Deliberately cleared, with the reason

These look like the concern and are not. Recorded so the next reader does not re-raise them.

- **Protocol replies (#2, #3).** A terminal that cannot answer a device-attributes query is broken.
  These bytes are responses to the program in the pane, never composed by TD.
- **Wheel-to-arrows (#4).** Standard behaviour for alt-screen programs; the human moved the wheel.
- **`seek_agent_prompt` (#7).** Walks an alt-screen agent's own scrollback. Two byte shapes, and the
  bound belongs to the shape, not to the walk: when the program holds the mouse it sends synthetic
  wheel notches (SGR `\x1b[<64;1;1M` or the X10 form) up to **400** steps, one row each; when the
  program never asked for mouse reports it falls back to PageUp/PageDown, capped at **24**, because
  each of those is half a screen. Arrows are deliberately not used — Claude Code binds ↑/↓ to prompt
  history, so an arrow would rewrite the composer instead of scrolling.

  It is the one write that outlives the gesture that started it: a detached `cx.spawn` loop that
  waits for a repaint between steps, so a single press can keep writing for tens of seconds. It
  stops early on three unchanged frames or when a human prompt scrolls into view. Synthetic,
  bounded, navigational, started by a keypress **or a click on ▲/▼** — it moves the view, not the
  cursor in a prompt.
- **The MCP server.** `set_pane_config` stores numbers for appearance. There is no verb that writes
  to a pty, and this audit confirmed the docs' claim rather than repeating it.

`td-send` used to sit in this list. It does not clear: it reaches site #8 over the control socket,
and it is written up in the next section instead.

---

## The one to keep an eye on

**Site #8** writes `"{cmd}\n"` into a freshly spawned shell — the `claude --resume …` line that pane
was running. It goes into a new shell rather than a live conversation, which is why it is not
grouped with keepalive. Four things arrive here, and they are not equally human:

- **Session restore.** You reopened a saved session. Human-initiated in the ordinary sense.
- **Resurrect.** You picked a dead agent out of the 💀 menu. Same.
- **`ctl adopt --run`.** A line on a unix socket. `scripts/td-send` sends it from a Hyprland hotkey,
  which is a person — but the socket does not know that, and anything running as this user can send
  the same line, **including an agent**: `plugins/td-send/` exposes `pull_workspace` as an MCP tool
  and says so in its own description. This is the one place where something other than a person
  causes TD to compose a command and press Enter.
- **`TD_SEED_RESUME`.** An environment variable read once at launch, documented in `--help` as "type
  that into the first pane". `td-send` uses it when no TD window exists yet to receive the tile.

It is the closest neighbour to the concern in this codebase, and the honest description of the
guarantee is narrower than "a human did it": what actually holds is that `adopt` **cannot name an
existing pane**, so the blast radius of the socket is a new shell in a new tab. `tmux new-session -d
'cmd'` over its own socket is the same shape, and a reviewer may still ask.

If that trade stops looking worth it, the walk-back is to spawn the shell and leave the line
**typed but unsent**, so restoring a session ends with a human pressing Enter. That change is four
lines in `new_restored` and costs `td-send` its one-gesture migration.

---

## Checking the invariant

The audit is a grep, and it should stay one. Each line below states the count it expects, because a
check whose expectation is wrong on the day it ships is a check somebody switches off:

```
rg -c 'self\.send\(' app/src/pane.rs        # expect: 1  — pane.rs:4099, inside on_key
rg -c 'notifier\.notify\(' app/src/pane.rs  # expect: 9  — the eight rows above, #4 twice
rg -c 'keepalive' app/src/main.rs           # expect: 1  — the module doc recording why it is gone
rg -n 'Msg::Input|notifier\.0|notifier:' app/src   # expect: nothing outside term.rs
```

The fourth line is the one that is easy to forget. `Notifier` is alacritty's newtype and its sender
is public — `pub struct Notifier(pub EventLoopSender)`, and `notify` is one line, `self.0.send(
Msg::Input(bytes))`. A future call site written as `notifier.0.send(Msg::Input(…))`, or a `Notifier`
cloned out to another module, writes to a pty while passing every other grep on this page. The
single primitive is a convention this repository keeps, not something the type system enforces.

A new `notifier.notify` call site is a change to this document, not just to the code. If you are
adding one, say in the pull request which row of the table it becomes and who initiates it.
