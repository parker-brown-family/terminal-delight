# Product: The workbench follows the agent, not the pane

## Problem

Parker ends an agent from its workbench and the workbench keeps that agent's
work on it. In his words:

> ending a session from workbench or killing the agent process really feels like
> it should wipe the history back down to the launch agent screen … currently
> ending a session in a pane workbench will keep the OLD agents workbench
> history (it is associated with the PANE) … it IS possible to associate the
> WORKBENCH with the AGENT ID ITSELF! … so that if a pane has NO agent — we show
> NO workbench. If a workbench resumes a DIFFERENT agent (if available) that
> agent's workbench transcript / history / comments / everything workbench is
> RE-LOADED back in.

Two things go wrong today, and they are the same mistake seen from either end.

Start the next agent in a pane where one has already worked, and it inherits a
bench full of somebody else's diagrams, replies and answered questions. There is
no gesture that gets rid of them — the only clean bench is a pane nobody has used
yet, so the way to start fresh is to close the pane and open another one, which
throws away the directory, the sticky note and the place in the layout as the
price of an empty rail.

Go the other way and the work is unreachable. A conversation that made eleven
surfaces this morning, ended at lunch, and gets resumed in the afternoon comes
back with nothing on its bench, because what it made was filed under the pane it
happened to be sitting in. Resume it somewhere else and the record does not
follow. The comments a reader left on those surfaces do not follow either.

Both fall out of one decision: a surface is currently remembered as *something
that happened in pane 7*, when what a reader means by it is *something my agent
made*. Panes outlive agents, get reused, and get closed; conversations are the
thing that has a beginning, an end, and a name.

## Success metric

**Zero surfaces on a bench belonging to a conversation the pane is not in**,
counted across every pane in a window. That number is measurable today —
`terminal-delight bindings` already prints which conversation each pane is bound
to and how strongly — and it is the number that was badly wrong on 2026-09-18,
when fourteen agents worked in one repository and eight benches showed a
neighbour's deliverables as their own. It has to stay at zero while the bench
starts reading its store by conversation instead of by pane, because that is
exactly the change that could push it back up.

**Rows on the rail of a conversation that has presented nothing: zero**,
including separators, folds and counts, on a pane whose directory holds any
number of surfaces from earlier conversations. Countable directly, and the one
number a regression here would move first.

Two more, both currently unbounded and both counted by hand:

- **Gestures from "the agent I was using has ended" to a bench ready for the
  next one:** today there is no number, because no gesture does it. Target: none
  at all — ending the agent is the gesture.
- **Gestures to get a finished conversation's work back onto a bench:** today
  there is no path. Target: at most two, from the pane it was made in.

## Announcement — the blog post before the feature

Your workbench now belongs to the conversation, not to the square it is drawn
in. End an agent and its bench goes with it, so the next agent you launch in that
pane starts on a clean rail instead of inheriting a stranger's diagrams. Nothing
is thrown away: bring that conversation back and everything it presented comes
back with it — the replies, the decisions you answered, the notes you left on
them — whichever pane you resume it in. A pane with no agent in it now shows what
is actually true, which is that there is nothing on the workbench and a button to
start something. And when Terminal Delight cannot work out which conversation a
pane is in, it says so and shows you nothing, because the alternative is showing
you somebody else's work with a straight face.

## Screens

- `mockups/ended.html` — the bench the moment its agent ends, in three
  treatments: a bare wipe, a wipe with one line pointing at the work that was
  kept, and a wipe that offers every resumable conversation. **Needs a decision.**
- `mockups/unknown.html` — the three states the wipe rule creates that are not
  "no agent": a live agent whose conversation cannot be named (D), surfaces no
  conversation can claim (E), and a brand-new conversation in a pane with a
  crowded directory behind it (F).
- A live agent's bench and a never-used pane's empty panel are unchanged, so
  neither gets a mockup.

## Decided at this gate (Parker, 2026-09-21)

1. **The ended state is treatment B** — the clean panel, plus one faint line
   naming how many surfaces the departed conversation left and a press that
   brings it back. A was rejected for stranding the work; C for putting the
   graveyard in front of a person whose wanted conversation is the one they were
   in a second ago.
2. **A surface no conversation can claim stays on the bench, below a line**, and
   is never listed among the live agent's own work or counted as the live
   agent's. This covers file drops — any process running as this user can write
   one, so the file can never name its writer — and every surface already on disk
   from before the change.
3. **A fresh bench shows none of it.** Parker, on seeing decision 2 drawn:
   *"We need to be sure that a FRESH NEW workbench doesn't show this nonsense!"*
   That splits the unclaimable surfaces into two groups, and only one of them is
   ever drawn:
   - **Arrived while this conversation was live, writer unknown.** The window
     watched it appear during a conversation it had bound. It did not see who
     wrote it, and it did see when — so it is drawn, below a line, labelled for
     what it is. Screen E.
   - **Found on disk, arrival unwitnessed.** Everything from before this change,
     and everything from earlier conversations in the same pane. **Superseded
     14:45 the same day** by Parker's note on the tenancy brief: these are not
     folded, not counted and not read. A pane holding them draws one screen
     saying the session is from a stale version of Terminal Delight, and that is
     the end of it. Screen G, and the reasoning is in `02-architecture.md`.

   A conversation that has presented nothing has **neither** — no rows, no
   separator, no fold, whatever else is on disk. A bench that opened with *"4
   surfaces belong to no conversation"* would be the old bug in smaller type.
   Screen F.

## Amendments taken at approval (Parker, 2026-09-21)

**The key is unique to the agent, and nothing else may reach that record.**

> we have had sort of race conditions where artifacts became jumbled … obviously
> we will avoid this scenario with a workbench is INCORRECTLY loaded and
> attributed to an agent … ie. imagine it is a UNIQUE ID key for the agent, and
> that no other agent can access that workbench file

Filing by conversation is not enough on its own; the store has to be arranged so
that reaching another conversation's record is not a thing that can be attempted.
Gate 2 answers how, and has to say plainly where the guarantee stops — a
guarantee that quietly holds only most of the time is the shape of the bug this
is meant to end.

**The person's own prompts are part of the record.** Today the bench can caption
only the newest reply with what was asked, and only while nothing is open off the
rail. Open any card and the block is gone. Parker:

> ALSO this 'YOU' field is default collapsed which is wrong — it should be
> default shown … if a human is reading a TURN, they should see what their
> prompt was

So a turn is the unit: what was asked, and what came back. Every reply carries
the ask that prompted it, for as long as the reply is kept, at whatever point in
the conversation it is read.

**The scrollback question, answered.** He asked whether the bench failing to
remember a human message is a scrollback problem. It was, exactly — the block
read the pane's history at paint time and came back empty on long turns, which he
called *"TOTALLY unacceptable, this is EXACTLY important"*. That was partly
repaired on 2026-09-19: the pane now latches each human turn off the scrollback
as it goes past, so the blank case is rare. What the latch holds is one message —
the newest — so it cannot caption anything but the standing reply, and that is
the same missing piece as the ask above. Persisting the ask per turn retires both.

## Open, and needing Parker before Gate 2

1. **Whether a live agent that cannot be bound shows nothing.** Every other
   reader in the app already refuses to guess in that situation. Extending the
   same refusal to the bench means an agent working in an unbindable pane
   presents into a screen that shows nothing, which reads as broken unless the
   screen explains itself. Recommendation: refuse, and say why on the screen,
   with the command that explains the refusal — `mockups/unknown.html`, screen
   D. Taken as decided unless Parker says otherwise.
