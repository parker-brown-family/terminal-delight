# The agent channel — TDAC 0.1

**Version 0.1 · 2026-09-21 · implemented in `app/src/channel.rs`, read by
`app/src/surfacefeed.rs`, written on the harness side by `scripts/td-agent-hooks`**

TDSP (`td-surface-protocol.md`) says how an agent hands the window a thing it
MADE. This document says how the workbench and the agent PROCESS talk to each
other about everything else: what the person typed, what the agent asked, what
the person answered, what the agent said back, and the two things a bench can do
to a process (interrupt it, end it).

```
   agent process          the channel                the workbench
  ───────────────       ──────────────              ───────────────
  a hook fires     →    inbound.jsonl        →      a card, a caption, a state
  a hook waits     ←    answers/<id>.json    ←      a press on a card
  its stdin        ←    one bracketed paste  ←      the composer's SEND
                        outbound.jsonl       ←      every one of the above, first
```

---

## 1 · The one rule

**The bench never reads the screen and never types a keystroke.** Everything it
learns arrives as a record with a type, a time and a source. Everything it does
is a record first and a delivery second. Where the record cannot be had — an
agent with no hooks, a window older than this protocol — the bench falls back to
the screen and to keys, **and says so on the card**. A fallback that looks like
the real thing is the failure this protocol exists to remove.

This is why the channel is asynchronous in the plain sense: nothing on the bench
waits for a terminal to echo. A send is done when the journal has it and the
bytes have left; an answer is done when the file exists; whether the agent took
either is a fact that arrives later, as another record, or does not arrive and
is drawn as *unconfirmed*.

---

## 2 · Where it lives

The pane's mailbox directory, which every agent already knows from its
environment (TDSP §2):

```
$XDG_STATE_HOME/terminal-delight/surfaces/$TD_SESSION/$TD_PANE_ID/
    *.json                    surfaces — TDSP, unchanged
    actions.jsonl             answers to surfaces — TDSP §7, unchanged
    inbound.jsonl             NEW · harness side → window. Append-only.
    outbound.jsonl            NEW · window → the record. Append-only. Window is the only writer.
    answers/<tool_use_id>.json  NEW · the window's answer to a question a hook is holding open
    bench.json                NEW · the window's liveness marker for this pane
```

**The inbox stays pane-addressed.** Gate 1 fixed this and nothing here moves it:
`TD_SESSION` and `TD_PANE_ID` are stamped at shell spawn, before any agent exists
to be named, and a hook is the agent's own child, so it inherits both. The
conversation key (`docs/plans/workbench-follows-the-agent/`) is a filing key
for the STORE and never an address.

Both journals are one JSON object per line, appended under an advisory lock
(`flock`) because a question with option previews is longer than a pipe buffer
and two hooks can fire inside one second. A reader keeps a byte offset per file
and reads only what is new. A line that does not parse is skipped, counted and
retried on the next sweep, the same rule the surface sweep uses for a file
caught mid-write.

---

## 3 · Inbound — what the harness side says

Every line carries `td`, `type`, `at_ms` (the writer's clock, used for ordering
within the file only) and, when the writer knows them, `pid` (the agent's
process) and `session_id`. Unknown types are **kept and counted**, never dropped:
a bench that silently ignored a record it did not understand would be the
transcript problem again.

| `type` | Written on | Carries | The bench does |
|---|---|---|---|
| `prompt` | the person submits a turn (`UserPromptSubmit`) | `prompt_id`, `text` | captions the overview with the EXACT words, and stops trusting the screen latch for this turn |
| `question` | the agent calls its question tool (`PreToolUse` on `AskUserQuestion`) | `tool_use_id`, `questions[]` verbatim from the tool input, `deadline_ms` | presents one `question` surface per question, whole round, provenance *hook* |
| `waiting` | the hook has decided to hold the picker and wait for the bench | `tool_use_id`, `until_ms` | routes the answer as a file while `until_ms` is in the future |
| `released` | the hook stopped waiting | `tool_use_id`, `why` ∈ `answered` `timeout` `stale` `no-bench` | routes any later answer as keys (the picker has painted) |
| `answered` | the tool returned (`PostToolUse`) | `tool_use_id`, `answers` — the map the tool returned, or `null` when the shape was not readable | marks the round's cards answered, whichever route the answer took |
| `reply` | the agent's turn ends (`Stop`) | `text` — the harness's own `last_assistant_message`, or `null` | if no `response` surface arrived this turn, presents one holding the reply as its plain brief, provenance *hook* |
| `notify` | the harness raises a notification | `notification_type`, `message` | records it; `permission_prompt` and `agent_needs_input` raise the pane's needs-you state without reading the screen |

`questions[]` is the tool's own shape, copied and not reshaped:

```json
{"question":"Which drink?","header":"Drink","multiSelect":false,
 "options":[{"label":"Tea","description":"…","preview":"…"}]}
```

`preview` is optional and long; it is kept. It is the part the screen reader
could never see.

---

## 4 · Outbound — what the window records before it acts

Written by the window only, before the delivery it describes. An agent may read
it; nothing waits on it.

| `type` | Carries | Delivered as |
|---|---|---|
| `say` | `id`, `text`, `delivery` ∈ `paste` `flat` `held` | one bracketed paste + `\r` into the process's stdin (§6) |
| `answer` | `tool_use_id`, `answers`, `route` ∈ `file` `keys` `sentence` | the file (§5), or keys, or the TDSP §7 line |
| `interrupt` | — | one `0x03` |
| `end` | — | two `0x03` in one write — asked to quit, not signalled, so the alternate screen comes down |
| `keys` | `bytes_hex`, `why` | raw bytes — the ONE impure verb, always journaled so the fallback is visible in the record |
| `launch` | as today's `launches.jsonl` | unchanged; listed so the vocabulary is in one place |

---

## 5 · The answer file, and the three routes

```
answers/<tool_use_id>.json
{"td":"0.1","tool_use_id":"toolu_…","answers":{"Which drink?":"Coffee"}}
```

Written beside and renamed into place. The map is keyed by the question's own
text and valued by the chosen option's `label`, because that is the shape the
harness's own answer channel takes: **measured 2026-09-21** by driving Claude
Code 2.1.274 under a pseudoterminal with a `PreToolUse` hook that returned
`updatedInput` carrying such a map — the picker never painted and the model
received the label. A multi-select answers with the chosen labels joined by
`", "`; that half is *inferred* from the tool's own description of its result,
not yet driven.

An answer takes the first route that is open, and the record says which:

1. **file** — a `waiting` for this `tool_use_id` is on record, its `until_ms` is
   in the future, and no `released` has followed it. Exact, silent, and the
   agent never draws a menu.
2. **keys** — the picker has painted (the screen reader supplied a `cursor`),
   so the same arrow-and-return bytes as before, through the same one gate.
3. **sentence** — neither: the `[workbench:<tag>] choose …` line of TDSP §7,
   for an agent to read on its next turn.

A person answering during the gap between a hook's timeout and the picker's
paint gets route 1 written and, when a cursor appears for the same question,
route 2 as well. Two deliveries of one answer are a bounded cost; a lost one is
not.

---

## 6 · Delivering a message

`say` is ONE write. When the terminal has bracketed paste on — every agent TUI
this house runs does — the bytes are

```
ESC [ 200 ~   <text, CRLF folded to LF>   ESC [ 201 ~   CR
```

so line breaks inside the draft stay line breaks and nothing inside the text can
submit early. When the terminal has bracketed paste off, line breaks become
spaces and the text is followed by a return, which is what the terminal face's
own paste does on such a terminal. The record's `delivery` field says which
happened.

The write goes through the pane's existing byte stream to the host, which is the
only process holding the pseudoterminal. That stream is the same one a keystroke
rides; what changed is the **unit** (a message, not a key), the **owner** (the
channel, not the key handler), and the **record** (a journal line, not a screen
echo). The visibility rules on a bench write are unchanged: on screen, an agent
in the pane, no dial press in flight; otherwise the message is held, in order,
and `delivery` says `held` until it goes.

---

## 7 · The marker, and why the hook needs it

```
bench.json
{"td":"0.1","face":"workbench","at_ms":1790006865309,"window":1983471}
```

The window refreshes this for every agent pane about once a second. The hook
reads it before deciding to hold a picker: it waits **only** when `face` is
`workbench` and `at_ms` is within four seconds of now, and it re-reads it while
waiting, so a window that dies mid-question releases the picker within four
seconds rather than at the hook's own timeout. A pane with no bench open, a
window older than this protocol, or a machine where no window is running all
read the same way — not fresh — and the picker paints exactly as it did before
this protocol existed.

---

## 8 · The adapter — `scripts/td-agent-hooks`

One script, one event switch, installed by `scripts/install-agent-hooks.sh`
into `~/.local/bin` and wired into `~/.claude/settings.json` (a `jq` merge that
appends and never clobbers; `--uninstall` reverses it). Its contract:

- **Exit 0 on every path.** A hook that fails must not break a person's session
  over a mailbox. Missing `jq`, missing `TD_SESSION`, an unwritable directory:
  exit 0, silently.
- **No `TD_SESSION`/`TD_PANE_ID` in the environment means the agent is not in a
  Terminal Delight pane**, and the script does nothing at all.
- **The wait is bounded twice**: by the marker's freshness (§7) and by
  `TD_ASK_WAIT_S` (default 540, under the harness's 600-second default timeout
  for a command hook, so the harness never kills the hook first).
- **It never reads a screen and never types.** Its output on the pre-answer path
  is the harness's own decision JSON; on every other path it is nothing.

Which hook events it is wired to, and what each yields:

| Event | Matcher | Yields |
|---|---|---|
| `UserPromptSubmit` | — | `prompt` |
| `PreToolUse` | `AskUserQuestion` | `question`, then `waiting` + the pre-answer, or `released` |
| `PostToolUse` | `AskUserQuestion` | `answered` |
| `Stop` | — | `reply` |
| `Notification` | — | `notify` |

The ledger hook (`td-agent-ledger`, `SessionStart`/`SessionEnd`) is a separate
script with a separate job and is not folded in.

Other harnesses: Codex CLI and Gemini CLI both have tool-lifecycle hooks; neither
has been driven against this adapter, and a question tool that can be
pre-answered has not been established for either. An agent with no adapter is a
**screen-only** agent, and the bench draws it as one.

---

## 9 · Provenance is drawn

Every record the bench shows names where it came from, using TDSP's `origin`:

| Origin | Means |
|---|---|
| `hook` | arrived through this channel; complete and exact |
| `derived` | read off the screen by the window; may be partial, may lag |
| `mcp` / `file drop` / `person` | as TDSP already defines them |

A question card reads *asked through the hook · whole round* or *read off the
screen · what was visible*. An overview caption reads the person's exact words
under `hook` and *as the screen showed them* under `derived`. This is the whole
difference between a decoupled bench and a scraped one, made visible where the
person is looking.

---

## 10 · What this is not

- **Not a wire change.** Nothing crosses the host socket. `hostproto` and
  `gridwire` are untouched, so a window carrying this protocol attaches to a host
  that predates it.
- **Not the store.** Which conversation a bench is showing, and where its record
  lives on disk, is `docs/plans/workbench-follows-the-agent/`. This channel
  writes into the pane's mailbox; that plan files from it.
- **Not a second surface format.** A question that arrives here becomes an
  ordinary TDSP `question` surface, drawn by the same renderer as one an agent
  declared or the window derived.

---

## 11 · Versioning

`major.minor`, the TDSP rule: a minor bump adds optional fields or new record
types and never re-cuts an existing one; a record naming a newer major is kept
as unknown and counted. **A field added for a bug is still a version bump.**

**0.1** — the first cut: seven inbound types, six outbound, the answer file,
the marker, the adapter.
