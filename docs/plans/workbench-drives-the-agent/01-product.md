# Product: The workbench drives the agent

## Problem

The workbench looks like a place you write. It is a window onto a place you
type. Those are different things, and the gap between them is where people get
hurt.

A person opens the bench, sees a bordered box with a caret in it, and treats it
the way they treat every other box like that: they arrow around, they highlight
a phrase, they press ctrl+c. Terminal Delight is not holding a document — it is
passing each keystroke through to a terminal it is drawing a picture of. So the
arrow moves a cursor nobody can see, the highlight does not exist, and ctrl+c
ends the agent's turn.

Parker, on that last one:

> if someone THINKS this is a text editor because it looks like a text editory
> and then then TD thinks it is a terminal because it is a mirror… the person
> will SHUT DOWN THEIR SESSION ACCIDENTALLy — I ahve had this painpoint in the
> past!

The same gap shows up in smaller ways all day. Shift+Enter breaks a line and
empties the box — "ouch - pain". A thousand-word prompt has 43% of itself
off-screen with no way to open the box further. Nothing you write can be
selected, copied or undone.

None of this is a missing feature. It is one decision showing through: the
workbench borrows the terminal's channel, so it inherits the terminal's meaning
for every key. The fix is to stop borrowing. **The workbench gets its own way of
talking to the agent process — a narrow, named, deliberate one — and stops being
a view of a terminal that happens to be editable.**

## What this is for, said once

A person should be able to write a long, considered prompt to an agent in a box
that behaves exactly like every other box they have ever typed into, and never
be able to lose their session, their draft, or their place by pressing a key
they have pressed ten thousand times somewhere else.

## Success metric

**Primary — gestures answered correctly: from 4 to at least 20 of 23.**

The brief's standard table is the scorecard, and today's row is measured:
4 gestures fully correct, 5 partly, 14 absent or wrong. The number is countable
by a person with the table in front of them and a pane open, and it is the
number this feature exists to move. Any gesture deliberately left out is named
in this document with its reason, so the denominator cannot quietly shrink.

**Structural — reachable divergences: 3 to 0.**

Today there are three known keystroke sequences after which what the box shows
and what the agent will receive are different strings, with nothing on screen
saying so: up-arrow, shift+enter, and clicking into a selection. The target is
zero, and zero *by construction* rather than by repair — there should not be a
second copy of the draft that can drift.

**Nothing to measure it against yet.** Neither number can be read from a running
build today, because the end-to-end harness runs the binary without a window and
the bench smoke rig can only stage shells. Making the first number readable is
part of the work, not a precondition for it.

## Announcement — the blog post before the feature

> **The workbench is now its own place to write.**
>
> Until today the box on the workbench was a picture of the terminal behind it,
> and every key you pressed went straight through. That made slash commands and
> history work for free, and it made everything else behave strangely: arrows
> moved a cursor you could not see, highlighting did nothing, and ctrl+c — the
> copy key everywhere else on your machine — interrupted your agent.
>
> Now the workbench holds your draft itself. Arrow keys move through your own
> lines. You can select with the mouse, extend with shift, copy, cut and undo.
> Shift+Enter starts a new line and the box grows to fit, both when you drag it
> and on its own when you paste in something long. The box sends when you send
> it, and not a moment before.
>
> Underneath, the workbench now talks to your agent through a small, deliberate
> interface instead of typing at it. Every part of Terminal Delight that wants
> to start an agent, send it something, or read what it sends back goes through
> the same door — and the terminal view goes on being exactly the terminal it
> always was, for when you want one.

## What the person gets, in order of how loudly they notice

1. **You cannot kill your session with a text-editing key.** Ctrl+C copies.
   Interrupting is a thing you aim at: the stop control on the strip, and one
   chord that is not the copy chord.
2. **Arrows walk your own lines**, and up at the top of the draft does not
   silently rewrite it with something you typed last week.
3. **Highlighting exists** — drag, double-click, triple-click, shift+click,
   shift+motion — and what you highlight can be copied and cut.
4. **Shift+Enter makes a line** and the box grows to hold it.
5. **The box opens up for a big draft**, both ways: you can drag it taller, and
   it opens on its own when a pile of text arrives. Parker: *"CLick drag to
   expand is neat, but it should definitely also AUTO expand if someone is
   dumping a PILE of text in…."*
6. **Undo works.**
7. **Pasted line breaks stay line breaks**, instead of being flattened to spaces
   so a stray newline cannot submit half of what you pasted.

## What this costs the person, stated plainly

Two things work today *because* the workbench types into the terminal, and both
are at risk the moment it stops:

- **Slash-command completion.** Typing `/mod` today raises Claude Code's own
  menu, because Claude Code is hearing the keystrokes. It will not be.
- **History recall.** Up-arrow into the agent's own history, same reason.

Neither is a detail — they are why the mirror was built. **Gate 1 does not
decide how they come back**, only that losing them silently is not acceptable.
Three shapes are possible and the choice is Gate 2's: the workbench asks the
agent for candidates over the same new channel; the workbench keeps its own
history of what *it* has sent; or the feature moves to the terminal face and the
workbench says so rather than pretending. The mockups show the first.

### The third cost, and it is larger than both — surfaced 2026-09-21

Raised by the pane building bench tenancy, and verified here against
`origin/main` rather than taken on report.

The workbench does not only *type drafts* at the terminal. It puts **raw bytes**
into the pane's pseudoterminal for two gestures that have nothing to do with
composing:

- **Answering a menu the agent is showing right now.** You press `2` on a live
  question card and a `2` goes down the pty, because the agent is blocked on
  stdin waiting for exactly that. `Dispatch::Keys { bytes, note }` exists for it.
- **Ending the agent.** The stop control sends two interrupts, deliberately, so
  the agent lowers its alternate screen and flushes its transcript on the way out
  — a signal would leave the pane reading as an agent forever.

Both go through `bench_deliver`, and `bench_deliver` writes to a terminal. It is
**synchronous with a terminal by nature**, and no amount of asynchrony hides
that: the far side is a process blocked on a file descriptor.

So the product question, in the person's terms:

> If the workbench stops typing at the terminal, can I still press `2` on a live
> question, and can I still end my agent from the bench?

**Three answers, and this one is not Gate 2's to make quietly.**

1. **Keep a narrow synchronous path.** The new API carries a "keys" verb
   alongside everything else, used only for a live prompt and for stop. Honest
   about the coupling, and small. The cost is that the boundary is no longer
   clean, and the one impure verb is the one everyone will reach for.
2. **Give it up.** A live question is answered on the terminal face, and the
   bench says so instead of offering a button that cannot work. Ending the agent
   moves somewhere that owns process lifecycle. Clean, and it takes a working
   gesture away from a person who has it today.
3. **Make the agent ask over the channel instead of over stdin.** The real fix
   and the most expensive one: a question becomes a surface with a reply address
   rather than a menu drawn on a terminal. It cannot be done unilaterally — the
   agent has to participate — so it is a direction, not a slice.

#### Evidence for option 3, and a fourth participant nobody has costed

Filed as **#619** by the pane working on the live-question path, and the
measurement is independently corroborated: during a multi-question round the
bench can only show the question the picker is currently painting, because
`live_questions` reads the terminal grid and the harness does not flush a
pending ask to disk. Their corpus: 44 `AskUserQuestion` calls across the 120
newest transcripts, 19 of them multi-question, **0 readable while pending** —
which matches an earlier independent finding of 0 of 43. The ask and its result
land as *adjacent lines* in the transcript, ten minutes apart in their specimen.

So option 3 is not only about a nicer answer path. **Today the workbench cannot
see the whole question**, and no amount of asynchrony fixes that by itself.

Their constraint, and it is a good one: option 3's "the agent has to
participate" may be false. A **PreToolUse hook on `AskUserQuestion`** could
publish the whole question set without the agent knowing the protocol exists —
which is the only path that works for an agent `derive.rs` was written to serve,
i.e. one that will never speak TDSP.

**Answered on 2026-09-21, read-only, by looking at what the wildcard hook has
already been handed rather than by adding one.** Two `PostToolUse` hooks are
registered with matcher `.*` — `herd hook` and `lean-ctx hook observe` — and a
wildcard fires for every tool in the dispatch path, so a hookable
`AskUserQuestion` has been reaching them since they were installed. No
`settings.json` change was needed to find out.

`~/.lean-ctx/context_radar.jsonl` is the store that answers. **An
`AskUserQuestion` record is in it, carrying the complete question set** — its
`content` parses to `{annotations, answers, questions}`, with every question,
every option and every option's preview text intact. So whatever observes that
tool is handed everything the bench would need, previews included.

The control that makes this readable: the radar is **unfiltered**, unlike
lean-ctx's three other stores. 25 distinct `tool_name` values in one window —
`Edit` 158, `Bash` 107, `Write` 68, `SendMessage` 28, `Read` 25. The
`sessions/`, `tool-calls.log` and `events.jsonl` stores are `ctx_*`-only, which
is why an earlier pass here concluded lean-ctx could not answer and **was
wrong**. herd's store does exist, at `~/.local/state/herd/state.json` rather
than under `share/`, and genuinely cannot answer: 25KB, live, and no tool names
at any level.

**What this does and does not prove.** It proves the tool is observable with its
full payload. It does **not** prove which hook phase delivered it, because the
radar does not record one: `event_type` is `mcp_call` for all 720 tool records
in the window, and the only other values are `user_message` and `session`. There
is no hook-phase field anywhere in it. The payload carrying `answers` says it
was written after the person answered, which is the `PostToolUse` signature —
but inferred from the payload's shape, not read off a label.

**So the open question narrows rather than closes:** does `PreToolUse` fire for
this tool, and hand `tool_input` *before* the picker paints? That is the only
part that matters for the bench, and it is the only part that needs a matcher in
Parker's `settings.json`. **That is his change to make** — it goes to him with
the evidence, not into a plan doc and not into a peer's worktree.

**Cite this with care, for two reasons that both produce wrong numbers.**

**It rotates in minutes, not hours.** `context_radar.jsonl` rolls into
`context_radar.prev.jsonl` at roughly 800 lines, and on 2026-09-21 that happened
three times inside one exchange: two readers an hour apart saw *disjoint*
`AskUserQuestion` records, then it rotated again four minutes later — mid-turn,
while one of them was writing the paragraph about it — so `.prev` took over the
second reader's record and the first reader's left both files. A third read
minutes after that found the live file down to **10 lines**. `_askhook.py`
returning zero is the expected steady state. Quote another reader's number with
their timestamp rather than re-running to confirm it, and snapshot anything you
intend to keep.

**Never `grep` it for a tool name.** `content` carries whole payloads including
prose that mentions tool names, so the bare string over-counts by roughly two
orders of magnitude. Measured on the same files: in `.prev`, the structured
`"tool_name":"AskUserQuestion"` count was **1** and the bare string **247** —
247×. In the live file the structured count was **0** and the bare string
**13**, so a grep reports thirteen hits in a file holding no records at all.
Parse the JSON. The reproducer that does it correctly, and that now prints
`hook phase recorded: NO` on every record so nobody re-derives the overclaim
from its output, is `reports/_askhook.py`.

Note what option 2 costs beyond the gesture: **ending the agent from the bench
is the gesture that starts the feature the tenancy work is building.** That is
not a reason to pick a different answer; it is a reason to pick one on purpose.

## Constraints on Gate 2 that Gate 1 already fixes

- **The inbox stays pane-addressed, even if the store does not.** Agents address
  surfaces by `$TD_SESSION` and `$TD_PANE_ID`, stamped into a pane's environment
  when the shell starts — *before any agent exists to be named*. That contract is
  published in the machine agent file, the launch briefing, the MCP catalogue and
  the `terminal-delight surface` command, and it is what this very session used
  to put its own surfaces on the bench. A conversation-keyed address is not
  available at the moment the variable has to be stamped. Gate 2 may re-key the
  store; it may not re-key the inbox.
- **Do not make accessibility harder.** It is out of scope by decision, not by
  accident. The only constraint it puts on the boundary is that a workbench which
  holds its own document is *closer* to exposing a text field to a screen reader
  than a mirror of a terminal grid ever was. Do not spend anything on it; do not
  build something that forecloses it.

## Explicitly out of scope, with the reason

- **Screen-reader and accessibility support.** Parker: *"I like this a bunch -
  and here is why…. one day we will have agnets doing phone calls and having a
  screen reader will be super for that --- do we do it now early before that
  problem is defined? NOPE!"* The new boundary should not make it *harder*
  later, and that is the only constraint it puts on Gate 2.
- **IME and dead-key composition.** Not established that it works today and not
  established that anyone here needs it. Named so the denominator is honest.
- **Rewriting the terminal face.** It stays exactly what it is. This work adds a
  door; it does not move the room.
- **The fifteen repairs to the mirror** the brief costed out. Superseded.

## The one thing that may not wait

The accidental-session-kill is a today problem, and the rework is not a today
fix. A stopgap exists — refuse to let the composer emit an interrupt byte, so
ctrl+c does nothing rather than ending the turn — and it is a handful of lines
that the rework deletes rather than inherits.

**This is Parker's call and it is asked as part of Gate 1 approval**, because
"don't step over our bounds" was explicit and a stopgap is a step over them.
The options are: ship the stopgap now, wait for the rework, or ship a narrower
one that only refuses the *silent* case and leaves a visible message.

## Screens

- `mockups/composer.html` — the workbench composer as a text box: a multi-line
  draft, a real selection, the expanded state, and where the two lost features
  (completion, history) surface if they come back over the new channel.
