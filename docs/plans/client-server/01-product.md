# Product: Client-server split

## Problem

When my terminal window dies — a crash, a redeploy, a fat-fingered close — every
running agent dies with it. I run long agent sessions that matter; losing the
window should cost me a window, not a day of running work. The window should be
the disposable part.

And the other face of the same problem: my automation keeps accidentally
*creating* windows. A command-line call that should be invisible instead boots
the full terminal and sprays scratch windows across the desktop (the 2026-09-08
phantom-window morning). Invisible things should stay invisible.

Both are one problem: today, the window process *is* the terminal. If it dies,
everything dies; if automation touches it, a window appears.

## Success metric

**Lost sessions = 0.** Scripted test: during a live agent run, kill and
relaunch the GUI N times; after every relaunch, every PTY, pane, tab, and
sticky note is still there and still running. The metric is the count of
losses across the run, and it ships at zero.

(Watched but not the metric: unexpected GUI windows spawned by a week of
automation — expected to fall to zero as a consequence.)

Amendments on approval (Parker, 2026-09-08): "nothing lost" explicitly
includes **scrollback** — the kill-relaunch script checks it — and
**non-resumable foreground programs** (vim, htop): they are children of PTYs
that no longer die, and the metric counts them.

### When the server itself dies

The layer under this feature does not go away: today's recovery — the session
TOML holding every pane's resume recipe, so agent sessions can be found and
restarted — remains the fallback for a server crash. Recovery follows the
agent session. Server death degrades to exactly today's behaviour, never
worse; the server existing just makes that path rare instead of routine.

## What closing means

Granularity decided at approval (Parker, 2026-09-08):

- **Closing a pane** kills that pane's session and process. A manual close is
  intent.
- **Closing a tab** — even one holding five panes — kills all its processes,
  for the same reason: it requires a deliberate action.
- **Closing the app** (or crashing it, or swapping its binary) kills nothing:
  every session keeps running on the server, and the next window comes back
  attached.

### Amendment, 2026-09-09: a close is deferred, not immediate

The three lines above still describe what a close *means*. What changed is
when it takes effect. Parker, reviewing the close path after the fresh-agent
review found that a final close never reached its save: *"maybe I accidentally
closed the tab and actually didn't mean to close it."* A manual close is still
intent, and intent is still what distinguishes it from the app going away — but
a person who meant to keep something gets a window in which to say so.

**How long depends on how much was closed, counted in panes rather than in
verbs.** Closing one pane holds it for **one hour**. Closing two or more panes
at once — which in practice means closing a tab, since a tab holding two panes
cannot be emptied by a single pane close — holds them for **four hours**. The
verb that did it does not matter: a one-pane tab closed as a tab is one pane
gone, and gets the one-hour window. Closing the app or the window expires
nothing at all, which is the feature this whole document exists for.

**The processes keep running for that whole window.** An agent mid-task whose
pane is closed is still working an hour later, and reopening the pane puts the
person back in front of it with its scrollback intact. Stopping an agent before
closing its pane is the person's job and stays a deliberate act; the terminal
does not decide that a close meant "and kill whatever that was doing." The cost
is real — held panes hold CPU, memory, and in an agent's case tokens — and it is
accepted here in exchange for the recovery.

**Retention is capped as well as timed.** The ten most recently closed panes are
held; an eleventh evicts the oldest immediately, whether or not its window has
run out. Without the cap an afternoon of ordinary churn holds a hundred process
groups, which is the same unbounded growth the host's own idle exit exists to
prevent.

**Two things are being retained, and they die at different times.** The live
process belongs to the host and goes when the host goes — a crash, a reboot, an
explicit shutdown, and it is not recoverable by any amount of bookkeeping. The
layout entry belongs to the session file and carries an expiry timestamp, so a
relaunch after the host has died still offers the closed pane back, restarted
from its resume line. Reopening must say which of the two the person is getting.
"Your pane is back, still running" and "your pane is back, started again" are
different facts, and a person who believes the first when the second is true
believes an agent kept working through the night when it stopped at midnight.

**Getting it back.** At relaunch, a held pane is the first candidate for a leaf
in the tree it was closed from — ahead of spawning anything fresh. Inside a live
session it takes an explicit key, most-recently-closed first, on repeat.

That key is **ctrl+shift+z**, and not the browser's ctrl+shift+t, which is
already Terminal Delight's own binding for a new tab (`app/src/main.rs:4`) and
every other Linux terminal's as well. Plain ctrl+t is not available to take
instead: inside a pane it belongs to whatever is running there — `transpose-chars`
in readline, the tag stack in vim — and a terminal emulator that claims an
unshifted control chord takes it away from every program in every pane with no
way for them to ask for it back. Undo semantics fit what this is anyway, since
the whole feature is an undo window on a close.

Nobody guesses ctrl+shift+z, so it needs to be told: a line in the F1 help, and
a one-time hint the first time a close is held. An undo nobody knows about is an
undo nobody uses.

**What this constrains downstream.** A closed-but-held leaf is a third state in
the saved layout, alongside live and gone, and it carries its deadline. It must
not be encoded as an absence, and it must not be encoded as an ordinary live
leaf — the second of those is what the branch does by accident today, and it is
why a closed pane gets replanned as a respawn on the next launch. Deferral rides
on `close_pane`, which is where the SIGHUP is and which the host owns; the
host's twelve-hour idle exit is longer than both retention windows, so the host
can hold the timer without the two rules fighting. And a pane the host is
holding for a person is not the same thing as a pane whose child exited on its
own — both leave an entry behind, and only one of them is waiting to be asked
for again.

## Announcement — the blog post before the feature

Terminal Delight now keeps your sessions alive even when the window isn't.
The terminal itself — every shell, every running agent, every pane and sticky
note — lives in a small always-on server on your machine; the window you see
is just a view attached to it. Close the window, crash it, or swap in a new
build mid-flight, and your work keeps running; open a new window and you're
exactly where you left off. Command-line and MCP tools now talk straight to
the server, so automation never pops a window again. Your terminal is finally
the place your work lives, not the process your work dies with.

## Screens

No new UI. The product is that the *existing* screens survive: relaunch the
app and the same windows, tabs, panes, scrollback, and notes come back live.
(Any small affordances — e.g. an attached/detached hint — are Gate 2+ details,
not product surface.)

## Out of scope (v1)

Two refusals and a parking, not omissions. All three were put on the table on
2026-09-08, and they are not the same kind of no.

**Remote attach.** v1 is this machine only; reaching a session from another
machine is a later feature, not carried in this spec. It cannot ride along
because the whole trust model today is the filesystem — a 0700 runtime dir, a
same-uid unix socket, and an `adopt --run` verb that runs arbitrary commands;
any transport that leaves that directory turns a paint-toggle surface into
unauthenticated remote code execution, and cross-surface auth is unsolved
(#284). Ownership is a local `flock` and addressing is pids plus `/proc`
parent-walks — none of it survives a network hop. Taking it now would mean
solving authentication before solving survival, and it moves the metric not at
all. Deferring it costs nothing later, on one condition: **"same uid, therefore
authorised" must not leak out of the transport into the protocol.** Keep
authorisation a checkable property of a connection, and keep one protocol core,
and remote arrives as a third transport plus an auth decision rather than a
rewrite.

**Multi-attach.** Several windows viewing the same live session at once is
explicitly not a v1 goal. If the architecture makes it cheap, fine — but
nothing in v1 is allowed to get harder to keep it possible. Unlike remote, the
price of reversing this one is not fixed: it is set by the Gate 2 grid-read
decision (server-side `Term` behind a generation/diff protocol vs. a
client-side replica fed the raw byte stream). The replica shape gives
multi-attach almost for free; keeping today's shared memory and single mutex
shuts the door, and re-opening it after Gate 3 means re-cutting the read path
under code already written against it. So the "nothing may get harder"
constraint has three checkable consequences for Gate 2: **nothing may assume
exactly one attached client** — resize is a fact the server is told, never one
it assumes; selection and scroll are named as client state even while only one
client exists; and the emulator's generation counter stays the
cache-invalidation token a second viewer would key on.

**The left-bar / workspace overhaul.** Not a refusal — a parking. Tabs-as-tasks,
the vertical project→epic bar, the collapsible left menu: Parker's own
direction, flagged highest priority the same day, and deliberately not carried
in this feature. It lives pinned in issue #319 ("Left bar: tabs and tab groups
go vertical; a tab becomes a task"). The constraint it leaves behind is the
same shape as multi-attach's: nothing in v1 may make it harder.
