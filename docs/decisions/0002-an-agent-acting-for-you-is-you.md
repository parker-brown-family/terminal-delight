# 0002 — An agent acting for you is you

Date: 2026-09-22
Status: accepted
Evidence: issue 686 (the gap, with the permissions and the `SO_PEERCRED`
asymmetry measured), pull requests 664 and 670 (which made a relay prove a
*window* and raised the question of the other direction)
Arises from: a security question asked while reviewing those two — *"is that a
security seam having MCP manipulatable like this?"*

The window's control socket takes a caller's identity on the caller's word.
`mcp from <session> <pane|-> rpc <json>` carries both fields, and neither is
checked against the process on the other end of the connection. **That is
deliberate.** Parker: *"these agents acting on my behalf should act like they
are me."*

An agent in a pane is not a subject to be authenticated. It is a hand. Asking
it to prove which pane it is would be asking one part of one person to prove
itself to another part of the same person, and there is no answer to that
question that means anything.

## What the check would and would not buy

`host.rs:390` already reads `SO_PEERCRED` to learn the pid on the other end of
its control connection, so the technique is in-house and proven. Not applying it
here is a choice rather than an oversight, and this record exists so the next
reader can tell those apart — silence reads exactly like the second one.

It would buy nothing, because the boundary it would enforce is not where the
boundary is. Measured 2026-09-22:

```
/run/user/1000            drwx------ parker:parker   tmpfs mode=700,uid=1000
/run/user/1000/cc-socks   drwx------ parker:parker
/home/parker              drwxr-x---
```

No other user on this machine can read, write, or traverse any of it, and
nothing here listens on a network. Every caller that can reach the socket at all
is already Parker. A peer-credential check would distinguish between processes
that are all equally him, and it would cost real things to do it: a pane-ancestry
check breaks for callers whose chain was legitimately reparented, which is the
whole subject of issue 215 (tmux), and the relay's resolution path is already the
part of this system that has most often been wrong about itself.

## What this does NOT say

**It is not a statement about content.** Tool results cross this socket and land
in an agent's context. A process that binds a `ctl-<pid>.sock` is a window to
any relay that resolves to it, and what it returns is read by an agent. That
channel is real, and the thing standing in it is the convention that agents
treat tool output as data rather than instruction — a behavioural control, not a
technical one, and not something this record makes safe.

Cooperative trust here is a statement about **identity**: who may speak. It says
nothing about **what** may be said, and the two should not be conflated by
anybody citing this record later.

## When this expires

This decision holds *because* every caller is already the same person on the
same machine. The direction this project is pointed breaks that premise, and it
breaks it on purpose:

- MCP servers that persist across shutdowns, so pane and agent identity outlives
  a restart
- Two Terminal Delight instances whose agents read across into each other's
  panes, and — under configuration — write a prompt into them
- Model-agnostic cross-agent messaging, where a Claude prompts a Codex or an
  open-weight model through this wiring rather than through a vendor's own
  channel
- An instance captured behind an exposed API, for a mobile or web access point,
  and for multiplayer

None of those is today's work. But **the first one of them that lets a caller
arrive from outside that `drwx------` directory ends this record.** A network
listener, a socket another user can reach, a relay accepting a connection it did
not spawn, a remote access point of any kind — at that moment the callers stop
being one person, "an agent acting for you is you" stops being true of all of
them, and identity has to be proven rather than asserted.

Whoever builds that must re-argue this rather than inherit it. The
implementation is not the hard part and already exists one layer down in
`host.rs`; noticing that the premise has changed is the hard part, which is why
it is written here rather than left to be rediscovered.

## What changed in the code

Nothing functional. The comment on the session comparison in `ctl.rs` said only
what it did, which let it read as an authorization check to anyone who arrived
at it cold. It now says which of the two it is: a guard against a relay that
resolved the **wrong** window, not against a caller that named the right one on
purpose.

Closes issue 686 as a documentation gap, which is the outcome its own third
invalidation criterion predicted.
