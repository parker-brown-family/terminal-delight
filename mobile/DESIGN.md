# Terminal Delight on the phone — design

How the phone app and its gateway are built, why, and what it would take to
hand them to someone who is not Parker. The plan that preceded the build is
`docs/plans/mobile/01-plan.md`; what shipped, slice by slice, is
`docs/plans/mobile/00-status.md`. This page describes the thing as it exists.

## The shape

```
 phone                               laptop
 ─────                               ──────
 Terminal Delight (Flutter)          td-mobile-gateway (Rust, user service)
   wall ─────┐                         │
   bench ────┼── HTTP + WebSocket ───▶ │── session-<key>.sock ──▶ session host ──▶ PTYs
   terminal ─┘   bearer token          │     (protocol v1, as kind "tool")
                 USB · Tailscale       │
                 · LAN (opt-in)        └── reads ~/.local/state/terminal-delight/surfaces/<s>/<p>/
                                           and ~/.config/terminal-delight/sessions/<s>.toml
```

Two programs. Nothing in the desk's window or session host changed to make
this work.

## Decisions

### The gateway is a client, not a new host verb

The session host already owns every terminal, and its protocol
(`docs/protocol/session-host-v1.md`) already says how a client lists panes,
starts one, attaches, and streams its bytes. The gateway speaks exactly that,
as one more same-user client. Two consequences drove the choice:

- **No host restart.** A running host is never restarted to ship a feature,
  so anything that needed a new host verb would have reached the phone only
  after the next reboot. The gateway worked against the host that was
  already running.
- **The host's security model stays local.** The host trusts a peer because
  it runs as the same user on a local socket, and nothing on its wire carries
  a credential. The gateway does its own authentication on its own
  boundary, and then meets the host as a local process. Nothing on the
  host socket ever learns about networks.

### The workbench reads files, not the window

Everything the desk's bench shows already exists on disk. Each pane's mailbox
holds its TDSP surfaces (`*.json`) and the agent channel's journals
(`inbound.jsonl`, `outbound.jsonl`); the session file holds the left bar's
projects, groups, tabs, colours and sticky notes. The gateway reads those and
never asks the window anything. That is why the phone works while no desk
window is open at all.

### Reading never takes a pane

The host gives a pane's byte stream to whoever attached last, and a desk
window that loses a stream keeps its last frame and stops until it is
reopened. So:

- the wall and the workbench come from files and take nothing;
- a terminal started from the phone is a new pane (`spawn-pane`) and takes
  nothing;
- attaching to a pane a window is showing is refused (409) unless the phone
  sends `take=true`, which only a tap on an explanatory sheet does;
- automatic re-attaches after a dropped connection never send `take`.

### The door

| Check | What it stops |
|---|---|
| Listens on loopback and the tailnet address only; `--lan` adds every interface | the home Wi-Fi seeing a plaintext door by default |
| Bearer token, 256 bits, compared in constant time, on every route but `/v0/ping` | anyone who is not a paired phone |
| Any request with an `Origin` header is refused (403) | a web page on the laptop reaching loopback |
| `Host` must name an address the gateway bound (421) | DNS rebinding |
| The pairing link is a proposal the person accepts on the glass | another app on the phone firing a forged pairing link |

The token lives in `~/.config/terminal-delight/mobile/token` (0600, in a 0700
directory). `td-mobile-gateway token --rotate` unpairs every phone.

### Routes

The pairing link carries every address the laptop might be reached at. The
app probes them all at once and uses the first, in preference order, whose
`/v0/ping` answers **as the paired machine**:

1. USB, `127.0.0.1:7717`, carried by `adb reverse`;
2. Tailscale, the laptop's tailnet address;
3. Wi-Fi, each private LAN address (only answers when the gateway runs with
   `--lan`).

The route is not fixed at launch. When the live feed drops twice on one route
the app probes again; with no route it keeps probing on a backoff; coming back
from the background re-checks. An open terminal whose connection drops without
a reason re-attaches to the same pane on whatever route is found. Measured on
the S21 FE on cellular: Tailscale through the Seattle relay at 177 ms, and a
terminal back on its pane within about a second of its connection being cut.

### The look

Next Run's Tactical HUD (the notched card, the grid ground, the staggered
rise) wearing Terminal Delight's Hacker palette. Each card's cut corner is
filled with the colour of the project or group the pane sits in on the desk.
The ground borrows the CRT's scanlines, vignette and rolling band, but not its
warp: text a thumb has to hit stays where it is drawn. Fonts are bundled —
JetBrains Mono (Nerd Font build) for the terminal and labels, Space Grotesk for
prose, Caveat for sticky notes, Noto Sans Symbols 2 for glyphs the phone's own
fonts lack.

## The gateway API, v0

Every route but `ping` needs `Authorization: Bearer <token>`. Errors are
`{"error": "<sentence>"}`.

| Route | Answers |
|---|---|
| `GET /v0/ping` | `{"gateway":"terminal-delight","v":0,"machine":"legion"}` |
| `GET /v0/hello` | the machine, its tailnet name, and every live session: `{key, host, panes, attended}` |
| `GET /v0/sessions/{key}/wall` | the host's pane list, each pane with `pulse` (state, why, last prompt, last reply, activity), `latest` (newest response card, cut down) and a surface count; plus `tree`, the desk's left bar |
| `GET /v0/sessions/{key}/panes/{pane}/bench` | the pane's host row, pulse, every current surface after `present`/`update`/`retire`, and the last 240 channel records, each tagged `side: in\|out` |
| `POST /v0/sessions/{key}/spawn` | body `{cwd?, run?, agent?: "claude"\|"codex", model?, cols, rows}`; answers the host's `spawned` reply. `agent: "claude"` starts Claude Code wearing the session's workbench briefing |
| `WS /v0/sessions/{key}/panes/{pane}/term?cols&rows[&take=true]` | binary frames are the terminal's bytes both ways; the phone sends `{"resize":{cols,rows}}` as text; the gateway sends `{"attached":N}`, then `{"closed":"ended"\|"taken"\|"host-gone"}` when the stream ends |
| `WS /v0/events` | hints, not data: `{"type":"mailbox",session,pane}`, `panes`, `tree`, `sessions`, `resync`. The phone refetches what it is showing. The gateway polls only while a phone is listening |

A pane's `pulse.state` is `needs-you`, `working`, `idle`, `ended` or
`unknown`. `unknown` means there are no channel records to read, which is
the honest answer for a pane with no hooked agent; it is never reported as
idle. Prompts the harness submits itself (background-task notices, subagent
hand-backs, cross-session messages) are flagged `harness: true` and never shown
as the person's words.

## Found on the device

Each of these was invisible until the app ran on the phone, and each is now
pinned by a test:

- **xterm.dart reads `ESC[>4;2m` as underline and faint.** Claude Code
  sends it to switch on modifyOtherKeys, and xterm.dart's SGR handler never
  checks the prefix, so everything after it drew underlined and dim.
  `SgrFilter` drops private-prefixed `m` sequences and normalises SGR 58/59
  and colon forms before xterm sees them.
- **The channel records the harness's own turns as prompts.** Measured across
  1,050 records: 315 task notifications, 86 cross-session messages, 5 subagent
  hand-backs. The desk's bench knows the first two; its gap is issue 846.
- **An agent with no file or MCP route prints its card as a ```td fence.** The
  phone lifts it into a card the way `app/src/derive.rs` does on the desk.
- **Removing an `adb reverse` rule does not cut connections already open
  through it.** Unplugging is tested by restarting the gateway.

## Who else could use this

The core generalises: it runs on any Terminal Delight install whose session is
hosted (the default since the client-server flip), speaks only documented
protocols, and reads only files every install already writes. What is shaped to
this one machine is the way a phone gets on, and one kind of agent.

| | Generalises | Specific to this setup | To open it up |
|---|---|---|---|
| Architecture | gateway as a plain protocol client, workbench from files | — | nothing |
| Pairing | the token and the confirm sheet | the link travels over **adb**, so a phone needs developer mode and a USB cable | a QR code the gateway prints, or a six-digit code typed on the phone, over any route |
| Reaching the laptop away from home | any Tailscale user gets it free | there is **no TLS**, so the only safe remote route is a VPN; `--lan` is plaintext | serve through `tailscale serve` for HTTPS on the tailnet, or add TLS to the gateway |
| Platforms | Flutter builds iOS | built and tested on **Android only**; the deep-link filter and adb pairing are Android | an iOS URL scheme, code pairing (above), a TestFlight build |
| Distribution | — | a sideloaded APK signed with a personal key | Play Store or F-Droid listing; the gateway packaged with TD (a verb on the main binary, or the AUR package) |
| Sessions | the gateway lists every live session | the app opens the session with the most panes and has no picker | a session chooser on the wall |
| Agents | anything with TD's hooks installed gets states and cards; any shell works as a terminal | "working" is sharpened by reading **Claude Code's** transcript; New offers Claude with the TD briefing; the harness-envelope list is Claude Code's | per-harness adapters, the way the agent channel already has one for Codex |
| Look | — | the phone wears Hacker, not the desk's theme | read the session's theme and map it onto the palette roles |

In short: another Terminal Delight user on Linux with Tailscale could build
and use this today, pairing over a cable. Handing it to people who are not
developers needs code or QR pairing, a TLS path, and a store build. None of
that changes the architecture.
