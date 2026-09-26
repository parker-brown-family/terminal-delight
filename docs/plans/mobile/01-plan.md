# Terminal Delight on the phone — the plan

*Written before the build. What was built, and why each decision held or
moved, is `mobile/DESIGN.md`.*

## What it is for

A person away from the desk should be able to see what every agent is doing,
read what each one last said, and take hold of a terminal when talking is not
enough. The desk already renders each agent's work as a **workbench**: the
pane's second face, a feed of response cards the agent writes into its mailbox.
The phone opens on that face. The terminal is one tap further in, because on a
phone keyboard reading is cheap and typing is dear.

## The shape

```
 phone (Flutter)                 laptop
 ───────────────                 ────────────────────────────────────────────
  wall  ─┐                        td-mobile-gateway ── session-1.sock ── host ── PTYs
  bench ─┼── HTTP + WebSocket ──▶    │  (hello, list-panes, watch, spawn,
  term  ─┘   bearer token            │   attach + byte stream)
             USB / Tailscale         └─ reads ~/.local/state/terminal-delight/surfaces/<s>/<p>/
                                        and ~/.config/terminal-delight/sessions/<s>.toml
```

**The gateway is a separate process that speaks the documented host protocol,
unchanged.** Two reasons. The running host is never restarted to ship a
feature, so anything that needed a new host verb would not reach the phone
until the next reboot. And the host's security model is the peer-uid check on a
local socket; the gateway authenticates the phone on its own boundary and then
meets the host as one more same-user client (`kind: "tool"`), so nothing on the
host socket learns about networks.

**The workbench needs nothing from the window.** Everything the desk's bench
reads is already a file: TDSP surfaces (`*.json`), the channel's
`inbound.jsonl` (the person's exact prompts, the agent's replies, its
questions, its needs-you notifications) and `outbound.jsonl`. The desk's tree —
projects, groups, tab names, colours, sticky notes — is the session file the
host writes. The gateway reads those and serves them.

## The door

| Decision | Chosen | Why |
|---|---|---|
| Where it listens | `127.0.0.1:7717` and the tailnet address only | USB rides `adb reverse` to loopback; Tailscale is WireGuard end to end. The home Wi-Fi is plaintext and shared, so it is off unless asked for with `--lan`. |
| Who may speak | a 256-bit bearer token, compared in constant time, on every route except `/v0/ping` | The phone is a new kind of client, and the host's own boundary (same uid on a local socket) says nothing about it. |
| How the token reaches the phone | `td-mobile-gateway pair` over USB: `adb reverse`, then a deep link the app shows as a *Pair with legion?* sheet | A deep link any app could fire is only a proposal; the person confirms it on the glass. |
| Browsers | any request carrying `Origin` is refused, and `Host` must be one of the addresses bound | A loopback port is reachable from every web page open on the laptop — the APES daemon's rule, same reason. |

## Taking a pane

The host gives a pane's byte stream to whoever attached last, and the window it
was taken from keeps its last frame and stops. That is correct for a window
that crashed and came back; for the desk it means a pane the phone reads live
is a pane the desk can no longer show until the window is relaunched. So:

- **Reading never takes.** The wall and the bench come from the mailboxes.
- **A terminal started on the phone takes nothing.** `spawn-pane` makes a new
  terminal on the host; the phone holds its stream, and the desk adopts it the
  next time a window opens the session.
- **Taking a desk pane is explicit**, behind a sheet that says what happens to
  the desk.

Both screens live on one pane at once needs a shared stream in the host. That is
slice 4, and it waits for a host restart that happens for its own reasons.

## The screens

1. **Wall** — the session drawn as the desk's left bar draws it: project,
   group, tab, in their colours. Each pane is a card: its name, what it is
   running, where, and its state — *working*, *needs you*, *idle* or *unknown*
   (a pane with no channel records is unknown, never idle).
2. **Bench** — one pane's feed: the latest response card open on its plain
   reading, the brief last, registers that unfold; the person's own prompts as
   the captions between turns; decisions and questions as cards.
3. **Terminal** — xterm on the phone, a key row for Esc, Tab, Ctrl and arrows,
   sized to the glass, resized on rotation and when the keyboard opens.
4. **Pair / connection** — which route is live (USB, Tailscale, Wi-Fi), the
   round-trip time, and the machine's name.

Look: Next Run's Tactical HUD (notched card, grid ground, staggered reveal)
wearing Terminal Delight's own palette and the session's project colours.

## Least confident

1. Whether xterm.dart keeps up with an agent TUI's redraw rate over Tailscale.
2. Whether a phone-sized pane (≈50 columns) is usable for Claude Code's TUI, or
   whether the terminal wants a landscape default.
3. The deep-link pairing sheet as the only confirmation step.
