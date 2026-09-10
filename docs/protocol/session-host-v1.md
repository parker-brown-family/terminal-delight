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

A connection that has sent `watch` also receives lines that answer no request,
tagged `push` where a reply is tagged `reply`. Nothing is pushed to a connection
that has not asked, so a client written before pushes existed still reads
exactly one line per verb it sends. A client that does ask has to tell the two
apart on that key before it parses any further — a push is not a reply, and a
reader that treats it as one will read one answer behind for the rest of the
session.

**Byte stream** — one greeting line naming a pane, then the terminal's own bytes
in both directions, unframed, FIFO. Nothing else is ever sent on it; a size is a
fact, not part of a terminal's output, and mixing the two means guessing where
one ends.

## Hello, and what a version means

`hello` asks whether the two sides speak the same protocol. It is **optional**,
and it opens nothing: every verb is answered on a connection that never sent
one. Real clients send it first because they want the answer, not because the
host requires it.

```json request
{"verb":"hello","proto":1,"kind":"window"}
```

```json reply
{"reply":"hello","proto":1,"session":"2","panes":2,"attended":true}
```

**The boundary is the peer-uid check above, and nothing else.** It is a property
of the connection, taken at accept, before a byte is parsed — which is what
makes it a boundary. A handshake would add no authority on top of it, and a verb
that trusted a claim inside its own payload would be a step backwards: `kind`
is such a claim, so `kind` decides nothing. It says what is talking, for a log
and for a person reading `list-panes`, and that is all it says.

Two consequences worth stating plainly, because an earlier draft of this page
said the opposite and code was written to match it:

- **A connection that never said hello may spawn, close, save and shut down.**
  This is the door a protocol bump walks out through: a new binary meeting an
  old host has to be able to say `shutdown` to a peer it can never negotiate
  with. Require a handshake and the refusal closes the repair with it.
- **A `window` hello takes nothing from anybody.** What arbitrates a pane is
  attaching to it: the most recent attach wins *that pane*, its previous holder's
  stream ends, and every other pane stays where it was. Two windows on one
  session share its panes and both stay live.

*(Reversed 2026-09-10. This page previously said `hello` was the first line of a
control connection and that a `window` hello superseded the previous window.
Neither was ever implemented; the reasoning for deleting rather than building
them is under "Amendment, 2026-09-10" in `docs/plans/client-server/02-architecture.md`.)*

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
| `hello` | negotiates a protocol version — optional, opens nothing | `hello` |
| `list-panes` | what exists, and what is true of it | `panes` |
| `spawn-pane` | start a terminal | `spawned` |
| `attach-pane` | set the size and declare intent to attach | `attached` |
| `resize` | tell a pane its new size | `resized` |
| `close-pane` | hang up a pane's process tree | `closed` |
| `grid-check` | what the host's own copy of a terminal hashes to | `grid-checked` |
| `save` | hand over the session's layout for the host to write | `saved` |
| `watch` | be told when something changes, on this connection | `watching` |
| `shutdown` | stop the host | `shutting-down` |

Anything unreadable, any unknown verb, and any version that cannot be spoken get
an `error` reply. Never a silent fall-through: silence is the failure mode this
whole feature started with.

```json request
{"verb":"list-panes"}
```

```json request
{"verb":"spawn-pane","cwd":"/home/parker/Work","resume":"claude --resume 48be90b8","geom":{"cols":100,"rows":30,"cell_width":8,"cell_height":16}}
```

`cwd` may be absent, and a directory that does not exist is ignored rather than
failing the pane.

`resume` is the line that puts an agent back in a conversation. **The host types
it, and the host will not type it twice.** Ask for a recipe this session is
already running and no terminal is started: the reply carries `started: false`
and the pane that is running it, to bind to instead.

That check belongs here and nowhere else. A client decides what to start by
comparing a saved layout against a list of panes it took a moment earlier, so an
agent can begin in a pane that list never showed — and the client starts a
second copy of it. Two agents on one conversation, both billing, both writing
the same transcript. Only the process that holds the pane table and performs the
spawn can check without a gap, which is why the recipe is sent here rather than
typed afterwards.

A pane's recipe is recorded the moment it is asked for, before its agent has
started, or a second ask arriving in that gap would find nothing. A pane whose
child has gone is not running anything, whatever it was started to run, and
asking for its recipe again starts a fresh terminal. A pane with no recipe is
deduplicated against nothing: two ordinary terminals are two terminals.

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
{"verb":"grid-check","pane":1}
```

```json request
{"verb":"save","schema":1,"body":"active = 0\n\n[[tabs]]\n\n[tabs.node.Leaf]\npane_id = 1\n","allow_shrink":false}
```

```json request
{"verb":"watch"}
```

```json request
{"verb":"shutdown"}
```

Every verb that changes something answers with an outcome — `{"ok":…}` or
`{"err":"…"}` — said truthfully. A queue acknowledgement says a message was
accepted, which is a different claim from anything having happened.

```json reply
{"reply":"spawned","started":true,"outcome":{"ok":{"pane":1,"shell_pid":40871,"cwd":"/home/parker/Work","resume":null,"mode":"shell","attached":false,"ended":false,"geom":{"cols":100,"rows":30,"cell_width":8,"cell_height":16}}}}
```

```json reply
{"reply":"spawned","started":false,"outcome":{"ok":{"pane":1,"shell_pid":40871,"cwd":"/home/parker/Work","resume":"claude --resume 48be90b8","mode":"claude","attached":true,"ended":false,"geom":{"cols":100,"rows":30,"cell_width":8,"cell_height":16}}}}
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
{"reply":"grid-checked","outcome":{"ok":{"pane":1,"stream_offset":14680,"hash":9257062766351139868}},"pane":1}
```

```json reply
{"reply":"saved","outcome":{"ok":{"written":{"leaves":3,"tabs":2,"merged":true}}}}
```

```json reply
{"reply":"saved","outcome":{"ok":{"refused-shrink":{"had_leaves":6,"had_tabs":3,"offered_leaves":1,"offered_tabs":1}}}}
```

```json reply
{"reply":"watching"}
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

## How long a host lives

A host stops on its own after **twelve hours in which nobody was attached and no
pane produced any output.** It checkpoints before it goes and says so in the
log. Two rules hold whatever else is true:

- **Never while a client is attached**, whatever the panes are printing. An
  attached window is proof the session is wanted. A connection that asked to
  `watch` counts too — it is a client, and a host that stopped underneath one
  would be ending a session somebody has open.
- **Checkpoint before going**, so what is left on disk is fresh rather than
  hours stale.

Twelve hours is generous on purpose. It needs no heuristic about whether a
silent agent is thinking, and a heuristic is what would eventually kill
something irreplaceable in a way nobody could reproduce. Before this, nothing
ended a host at all — which was never a decision anybody made.

Nothing on the wire announces it. A client finds out the way it finds out about
any host that has gone: its connection closes, and the socket is no longer
there.

## Closing, and what does not close

`close-pane` is intent, and intent is the only thing that kills: it hangs up the
pane's whole process tree, the way a terminal window closing always has. A
client disconnecting, crashing or being superseded kills nothing. That
distinction is the entire product.

**A pane's byte stream is open only while its child is alive.** When the program
inside a pane exits, the host sets `ended` and then closes the stream — in that
order, and the order is load-bearing. A client attaching to a pane whose child
has already gone is sent the final screen and then hung up on, by the same rule
arrived at through the other door.

A closed stream means one of two things, and a client must not guess between
them: the program ended, or another window took the pane. **Ask.** `list-panes`
answers it — a pane still listed and not `ended` was taken, and a pane that is
`ended` or gone from the table has really finished. Because `ended` is set
before the hangup, and the hangup is what closes the stream, a client can never
see the close and then be told the pane is fine.

The exit *status* is not on the wire. A hangup cannot carry one, and the push
that would is not in this version.

## What the host does with nobody watching

Two questions can only be answered by the process holding a pseudoterminal, so
both are the host's now:

- **The foreground watcher** asks `tcgetpgrp` what is running in each pane,
  publishes it as `mode`, and pushes the change to any connection that asked. An agent keeps its name through the child processes it
  runs — bash, node, rg — for as long as the alternate screen is up, because a
  pane that renames itself twice a second is worse than one that is a beat
  behind. When the agent exits and the plain shell returns on the normal screen,
  the demotion is real.
- **The checkpoint** reads each pane's live working directory and its agent
  resume recipe and keeps them, so `cwd` and `resume` are readings rather than
  whatever was requested at spawn. It also reads a pane the moment the watcher
  sees it change what it is running, because that is when those two facts change
  and something may ask for them before the clock comes round — a window
  planning an attach reads `list-panes` before anything it does wakes the host.
  A reading that could not be taken never replaces one that was: only an answer
  replaces an answer.

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

## Who writes the session file

**The host does, and nothing else.** It holds the pseudoterminals, so it is the
only process that can say where a pane actually is or what would resume the
agent inside it — and one writer is what stops two processes with different
ideas of the tree taking turns overwriting each other.

A window hands its layout over with `save` and the host writes it:

- `body` is TOML and **the host does not read most of it.** It fills in `cwd`
  and `resume` on the leaves carrying a `pane_id` it is running, and writes
  everything else back exactly as it arrived. A host that parsed the body into a
  type of its own would drop every field written by a client newer than itself,
  and the loss would surface later as settings quietly reverting. A leaf with no
  `pane_id`, or one naming a pane this host does not run, is left alone.
- `schema` says what shape the body is. A shape this build does not know is
  written through untouched rather than refused — a save that lands without
  fresh directories loses a little, one refused loses the lot — and the reply
  says which happened, in `merged`.
- `leaves` and `tabs` in the reply are what the **host counted by walking the
  tree**, never a number the body claimed. The count it writes back into the
  file is the same one, because session ranking reads that integer without
  parsing the tree and a stale value there decides which session a cold launch
  reopens.
- `allow_shrink` is the client saying a tree that lost most of its panes lost
  them on purpose. Absent means no. A save that would halve a session of any
  size is refused, the file on disk stands, and the reply says what was on disk
  and what was offered. The saves that shrink a session by accident are the ones
  nobody asked for.

The host also writes on its own, every 30 s, with or without a window: the
layout it holds, with fresh directories merged in. That is what makes a crash
cost recency rather than the layout. It seeds that layout from the file on disk
when it starts, so a host nobody has spoken to yet still has something truthful
to write.

**Single writer does not mean sole writer.** The file is a plain document in a
directory a person can open, and the documented way to recover a bad save is to
copy a backup over it. So before each checkpoint the host looks: if the file has
changed since it last left it, the host carries on from what is there instead of
writing its own copy over it. A file that will not parse is left for the next
tick — half a write is not a layout — and a `save` from a window still wins over
whatever is on disk, because a window is showing the live tree and that is a
better account of the session than any file.

## Being told, instead of asking

A window that has sent `watch` is told when a pane changes what it is running,
rather than asking every 800 ms for an answer that is usually the same:

```json push
{"push":"mode","pane":1,"mode":"claude"}
```

On the change, never on a clock. The current mode of every pane is what
`list-panes` is for, and a window that has just attached should read it there
once rather than wait for something to move.

`watch` is a verb rather than a field on `hello` because a client written before
pushes existed reads one line for each verb it sends, and a line it did not ask
for is an error to it. A connection that has not sent `watch` is never pushed
to, so adding this broke nothing and enabling it is a decision a client makes.

A push is written with the same care as a reply and then forgotten about. It is
never retried, and a connection that cannot take one within a quarter of a
second loses its subscription — the writing happens on the host's own watcher
thread, and a clock that one wedged window can stop is not a clock. A client
that suspects it has missed something asks `list-panes`, which is always the
truth.

## Leaving a full-screen program

A snapshot can only read the grid that is active. So a client attaching to a
pane that is running `vim` or `htop` is sent the alternate screen and nothing
behind it: its scrollback starts empty, its primary grid blank, and no amount of
live output will fill them, because that history was written before it arrived.

When the program exits, the host sends that client a fresh snapshot — the same
bytes an attach sends, under the same fence — and the history it never saw
appears behind the screen it was watching. Nothing is asked for and nothing is
lost; the pane's own `?1049l` has already reached the client through the byte
stream, so the paint lands on the primary grid where it belongs.

The divergence check below is the backstop if this is ever missed. This is the
part that makes it not need one.

## Checking that a client's copy is still the same terminal

A client draws from its own copy of the terminal, fed by the byte stream. Copies
drift — a dropped chunk, a resize applied at a different byte position — and a
drifted copy shows a person something their terminal does not contain. So the
host will state what its own grid hashes to:

- `hash` is over every scrollback and screen cell, the cursor, and the modes a
  snapshot restores. It deliberately excludes the scroll position and the
  selection, which belong to the viewer: a client may be scrolled back or
  holding a selection and still be a faithful copy.
- `stream_offset` is how many bytes had been written to *this client's* stream,
  the opening snapshot included, when that hash was taken. Both numbers come
  from inside the same fence the handover uses, so they describe one moment.

The offset is what makes the comparison mean anything. A client whose own read
count is behind the stated offset has not caught up yet, which is a different
finding from disagreeing, and a guard that could not tell them apart would call
for a repair on every busy pane.

A pane nobody is reading answers `err`. There is no stream, so there is no
offset into one, and a zero would be a number a client could compare against and
be confidently wrong about.

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
