# The session host protocol, version 1

A session host owns the pseudoterminals of one Terminal Delight session. Windows
are clients: they attach to panes, draw them, and are allowed to die without
taking the work with them. This page describes protocol version 1 — the words
those two say to each other.

**This page is executable.** Every fenced `json` block below is parsed by
`app/src/hostproto.rs`'s conformance tests, which also refuse a block naming a
field the host does not have, and refuse a verb or a reply the host can speak
that is not documented here. The document is what the host is tested against, so
it cannot quietly become fiction.

## Where it lives, and who may speak

One socket per session:

```
$XDG_RUNTIME_DIR/terminal-delight/session-<key>.sock
```

Without `XDG_RUNTIME_DIR` the directory is `/tmp/terminal-delight-<uid>`. Either
way it is the same private, `0700` directory the existing control sockets use,
created by whoever binds first.

Authorisation is the directory, confirmed at the moment of connection by
`SO_PEERCRED`: a peer whose uid is not ours is dropped before a single byte is
read. **Nothing on the wire carries a user, a token or a capability**, and that
is deliberate. Authorisation is a property of the connection, and the moment it
becomes a field in a message, every future transport inherits a security model
that was only ever true of a local pipe.

The host stamps two variables into every pane it starts, so a program inside a
terminal can say which session and which pane it is in without walking `/proc`
and guessing:

- `TD_SESSION` — the session key.
- `TD_PANE_ID` — the pane's durable id.

## Framing

Two kinds of connection, told apart by the first line.

**Control** — one JSON object per line, newline framed. One reply per request,
in order, on the same connection. Newline framing because every other seam in
this codebase already uses it, and JSON because the alternative is a binary
format nobody can read in a log at three in the morning.

**Byte stream** — one greeting line naming a pane, then the terminal's own bytes
in both directions, unframed, FIFO. Nothing else is ever sent on it; a size is a
fact, not part of a terminal's output, and mixing the two means guessing where
one ends.

## Hello, and what a version means

`hello` is the first line of a control connection.

```json request
{"verb":"hello","proto":1,"kind":"window"}
```

```json reply
{"reply":"hello","proto":1,"session":"2","panes":2,"attended":true}
```

`kind` says what is talking. A `window` may attach, and attaching supersedes
whoever held the pane. A `tool` — a probe asking which sessions are alive, a
script listing panes — connects, asks and goes, and must never cost a live
window its stream. That is safe by construction rather than by a check somebody
has to remember: attaching is a property of opening a byte stream, and a control
connection never opens one.

`attended` is whether any window currently holds a pane's stream. A host with
panes and nobody looking at them is exactly what a relaunch should find and
adopt.

**The versioning rule.** The number is bumped only for a change an older peer
would *misread*. Fields may be added within a version, and both sides ignore
what they do not recognise — that is what lets a client and a host from adjacent
builds keep working. A peer speaking a version this build cannot is refused, by
name, so nobody has to read a changelog to work out which side is old:

```json reply
{"reply":"error","msg":"protocol mismatch: this build speaks 1, the other side speaks 2"}
```

## The verbs

| Verb | What it does | Reply |
|---|---|---|
| `hello` | opens a control connection | `hello` |
| `list-panes` | what exists, and what is true of it | `panes` |
| `spawn-pane` | start a terminal | `spawned` |
| `attach-pane` | set the size and declare intent to attach | `attached` |
| `resize` | tell a pane its new size | `resized` |
| `close-pane` | hang up a pane's process tree | `closed` |
| `shutdown` | stop the host | `shutting-down` |

Anything unreadable, any unknown verb, and any version that cannot be spoken get
an `error` reply. Never a silent fall-through: silence is the failure mode this
whole feature started with.

```json request
{"verb":"list-panes"}
```

```json request
{"verb":"spawn-pane","cwd":"/home/parker/Work","geom":{"cols":100,"rows":30,"cell_width":8,"cell_height":16}}
```

`cwd` may be absent, and a directory that does not exist is ignored rather than
failing the pane.

```json request
{"verb":"attach-pane","pane":1,"geom":{"cols":100,"rows":30,"cell_width":8,"cell_height":16}}
```

```json request
{"verb":"resize","pane":1,"geom":{"cols":120,"rows":40,"cell_width":8,"cell_height":16}}
```

```json request
{"verb":"close-pane","pane":1}
```

```json request
{"verb":"shutdown"}
```

Every verb that changes something answers with an outcome — `{"ok":…}` or
`{"err":"…"}` — said truthfully. A queue acknowledgement says a message was
accepted, which is a different claim from anything having happened.

```json reply
{"reply":"spawned","outcome":{"ok":{"pane":1,"shell_pid":40871,"cwd":"/home/parker/Work","resume":null,"mode":"shell","attached":false,"ended":false,"geom":{"cols":100,"rows":30,"cell_width":8,"cell_height":16}}}}
```

```json reply
{"reply":"attached","outcome":{"err":"no pane 9"},"pane":9}
```

```json reply
{"reply":"resized","outcome":{"ok":null},"pane":1}
```

```json reply
{"reply":"closed","outcome":{"ok":{"shell_pid":40871,"signalled":true}},"pane":1}
```

```json reply
{"reply":"shutting-down"}
```

`signalled` is whether the hangup actually reached a process group. `false`
means the pane was already gone, which is not a failure and is not a kill
either.

## What the host knows about a pane

```json reply
{"reply":"panes","panes":[{"pane":1,"shell_pid":40871,"cwd":"/home/parker/Work/terminal-delight","resume":"claude --resume 4a1c…","mode":"claude","attached":true,"ended":false,"geom":{"cols":100,"rows":30,"cell_width":8,"cell_height":16}},{"pane":2,"shell_pid":40903,"cwd":"/tmp","resume":null,"mode":{"other":"vim"},"attached":false,"ended":false,"geom":{"cols":80,"rows":24,"cell_width":8,"cell_height":16}}]}
```

| Field | Meaning |
|---|---|
| `pane` | the durable id. Minted by the host, monotonic per session, never reused. **Not a pid** — a pid is the address of a thing that dies and gets recycled, and addressing panes by pid is where four separate session cross-wiring incidents came from |
| `shell_pid` | the process inside it, reported as the mere attribute it is |
| `cwd` | where the pane actually is, as of the last checkpoint |
| `resume` | the line that would put an agent back in the conversation this pane was having |
| `mode` | what is in the foreground: `"shell"`, `"claude"`, `"codex"`, `"remote"`, or `{"other":"vim"}` for anything else, named |
| `attached` | whether a window holds this pane's byte stream |
| `ended` | whether the process inside it has gone |
| `geom` | its size, in cells and in pixels per cell |

**`null` means the host has not read it, and never means zero.** A pane whose
working directory has not been read yet is not a pane in the root directory, and
a pane nobody has classified yet is not a pane running a shell. A client may
show a dash; it may not show a default.

## The byte stream, and the handover

A window that has said `attach-pane` opens a second connection and greets it
with the pane it wants:

```stream
stream 1
```

Everything after that line is the terminal's bytes: keystrokes up, output down.

The host answers by writing a **snapshot** — the pane's scrollback, screen,
colours, cursor and modes as VT bytes another terminal can eat — and then the
live stream, with nothing lost or doubled between them. The seam is closed by
taking alacritty's terminal *lease* across both, so every byte falls on exactly
one side: read before the fence and therefore already in the snapshot, or read
after it and therefore sent live.

**The newest attach wins.** A window relaunching after a crash must not be
refused by the ghost of the window it is replacing, so a second stream on the
same pane supersedes the first, whose socket is closed. Nothing here assumes
exactly one client has ever existed: each attachment carries a serial, and a
superseded client leaving releases only what is still its own.

A client that stops reading is dropped rather than allowed to stall the
emulator — its socket closes, and it re-attaches, which costs a snapshot and is
always correct, because a snapshot is the truth.

## Closing, and what does not close

`close-pane` is intent, and intent is the only thing that kills: it hangs up the
pane's whole process tree, the way a terminal window closing always has. A
client disconnecting, crashing or being superseded kills nothing. That
distinction is the entire product.

## What the host does with nobody watching

Two questions can only be answered by the process holding a pseudoterminal, so
both are the host's now:

- **The foreground watcher** asks `tcgetpgrp` what is running in each pane and
  publishes it as `mode`. An agent keeps its name through the child processes it
  runs — bash, node, rg — for as long as the alternate screen is up, because a
  pane that renames itself twice a second is worse than one that is a beat
  behind. When the agent exits and the plain shell returns on the normal screen,
  the demotion is real.
- **The checkpoint** reads each pane's live working directory and its agent
  resume recipe and keeps them, so `cwd` and `resume` are readings rather than
  whatever was requested at spawn.

Both back off when nobody is watching. A host outliving its window is the point
of this feature, so a fleet of headless hosts polling at window speed would be
the bill for it:

| Clock | Attached | Detached |
|---|---|---|
| foreground watcher | 800 ms | 5 s |
| checkpoint | 30 s | 5 min |

Attaching restores the attached cadence at once rather than at the end of a
five-minute sleep: an attach rings a bell both loops are sleeping on. Spawning a
pane rings it too, so a pane that has just appeared is read straight away rather
than at the end of whatever period was already running.

A reading the kernel would not give never overwrites one it did. A pane whose
foreground group cannot be read keeps the last mode actually seen, and a
checkpoint that fails to read a directory leaves the last known one in place.

**The session file is still written by the window.** It carries window bounds,
tab names and a theme, none of which a host has ever seen. What moved here is
the half that stopped being answerable from a window at all; the verb that hands
the layout over for the host to merge and write arrives with the attaching
client.

## Invariants, and the tripwires that hold them

- **The lease is the fence.** Alacritty's reader holds a lease for its whole
  cycle, so a lease taken here cannot overlap one. The fair `lock` takes the
  lease too — holding a lease and then calling `lock` deadlocks against
  yourself, which is why the snapshot pairs a lease with `lock_unfair`, exactly
  as alacritty's own reader does. **Re-verify this on any alacritty upgrade.**
- **The host's pane table beats the session file.** A live pane the saved
  layout does not claim is adopted, never dropped.
- **Ids are never reused**, so a message naming a dead pane is answered with an
  error rather than acted on against a live one.
- **Scrollback never goes to disk.** It lives in the host's memory and travels
  as a snapshot.

## Conformance

Pinned in `app/src/hostproto.rs`:

- `every_json_example_in_the_protocol_page_parses` — every `json` block above
  deserialises into the type its fence declares.
- `the_page_never_names_a_field_the_host_does_not_have` — a key in an example
  that the host would not emit fails, which is what serde's tolerance of unknown
  fields would otherwise hide.
- `every_verb_and_every_reply_is_documented` — the variant list comes from serde
  itself, so a verb added to the host and not to this page fails.
- `every_field_the_host_emits_appears_on_the_page` — the same in the other
  direction, over the fields.
- `the_page_states_the_version_this_build_speaks` and
  `the_page_quotes_the_refusal_this_build_writes` — the version number and the
  mismatch wording are the code's, not a paraphrase.
- `the_documented_stream_greeting_is_the_one_the_host_parses` — the greeting
  block above is fed to the real parser.

And in `app/src/host.rs`, over real pseudoterminals: the watcher classifying a
live pane, the checkpoint reading a pane's actual directory, unknown never
overwriting a reading, and a detached host backing off and being restored by an
attach within one wake.
