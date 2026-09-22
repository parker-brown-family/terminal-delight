# Status: the turn in flight is a card

**Difficulty: 5/10** — one new state on the bench (a turn that has begun and
presented nothing yet) threaded through the three readers that decide what the
room shows, plus a renderer. Wrong for a day would mean the overview captioning
a live turn with the previous turn's answer, which is the defect this fixes
pointing the other way; the unwind is deleting one field and its three readers.
4–6 buys one combined plan page and one approval, and this is it.

## The brief, 2026-09-21

Two screenshots — a bench mid-turn, and the terminal face of the same pane —
with:

> when the CURRENT turn is running in workbench... we see the LAST turn
> persisting -- the CURRENT turn should IMMEDIATELY make a new overview card,
> and we flip to that - and it hos the HUMAN prompt -- and whatever placholder
> -- maybe the verbose current turn of the agent working -- but only the action
> that is LIVE -- ie. not the terminal scrollback style.

## What was found

The overview's room is `Bench::showing` — the opened card, or *the newest
`response` surface* when nothing is opened. A `response` only exists once the
agent presents one, which happens at the END of a turn. So for the whole length
of a turn the newest response is the PREVIOUS turn's, and the rail's top row
wears `STANDS NOW` over it. The bench was telling the truth about its store and
lying about the conversation.

The person's own words were already right: `ask_lines` draws the YOU block over
the stand-in, latched off the pane's scrollback or handed over exactly by the
`UserPromptSubmit` hook. What was missing underneath it was the turn itself.

Every input the card needs already exists and is already parsed:

| fact | where it comes from |
|---|---|
| a turn began, with the person's words | `channel::Inbound::Prompt` → `Effect::Asked` |
| a turn began, opened by the harness | the same record → `Effect::Woken` |
| a turn began, no hooks on this pane | the not-working → working edge in `accrue_tokens` |
| the live action | `hud::AgentStatus::gerund` / `::doing`, `tool_face.verb` |
| the turn's clock and tokens | `workbench::turn_vitals` |
| the reply landed | `pane::present` seeing a `Kind::Response` |

## Decisions taken without asking (amber — argue with any of them)

- **A turn in flight is NOT a surface.** It is `Bench::live: Option<LiveTurn>` —
  nothing was presented, nothing goes in the store, nothing survives the
  process. It gets a rail row and the room, and no actions, tabs or registers.
- **It is retired by a reply arriving, never by the agent going idle.** A timer
  would be a guess and idle-retirement would put the previous turn's card back
  in the room for the gap between the turn ending and its reply landing — which
  is the reported defect, in miniature. A turn that ends having presented
  nothing keeps its card and the card SAYS so.
  **Amended the same sitting** — and by the same rule the decision rests on.
  The card first said `NOTHING PRESENTED · this turn ended without presenting a
  reply`, which is a claim about the future made at the one moment it cannot be
  checked: `Done` is raised off the finish bell on the pane's own 120ms clock,
  the reply comes from the harness's stop hook on its own, and the two race on
  EVERY turn. It now says `THE TURN ENDED · no reply has landed on the bench` —
  both halves observable right now, both still true if nothing ever comes. Only
  `AGENT GONE` may be final, because a dead process is a fact rather than a
  forecast. Parker, asked whether this decision was right: *"gets tricky and
  grey - but let's leave it in assumption land right now"* — so the RULE is
  parked as an assumption, and the sentence it made the card say is not.
- **Whose voice opened it decides whether an opened card is taken.** A person
  who typed has moved themselves, so their turn clears the selection and the
  room flips to it. A wake-up has not moved them and does not — *a place Parker
  put himself is his until he moves it*. A prompt record carrying no text leaves
  the voice **unknown**, and unknown does not take the room either.
- **`STANDS NOW` becomes `IN FLIGHT` on that row.** A turn that has said nothing
  yet does not stand for anything.
- **The row cannot be opened.** `select` on the live id closes whatever card is
  open instead — which is what "go to the turn in flight" means — and
  `set_shelf`'s auto-select skips any row that names no surface.
- **The card draws the LIVE action only**: the agent's own gerund, its clock,
  its tokens, and the one tool line it is inside right now. No scrollback. The
  terminal face is three keystrokes away and is the place that keeps the log.

## Slices

- [x] `LiveTurn`, `Voice`, and the four verbs on `Bench` (`turn_began`,
      `turn_seen_working`, `turn_settled`, `live`)
- [x] the three readers: `showing`, `standing_in`, `rows_for` + `counts`
- [x] `live_says` — the words, as a pure function a test can read
- [x] `benchdraw::live_card` and the `IN FLIGHT` lane word
- [x] wiring: the prompt record, the working edge, the reply arriving
- [x] tests, including a mutation pass on the retirement rule
- [x] **looked at on a screen** — a throwaway window on the unfocused monitor,
      driven by a stand-in named `claude` that paints the exact shapes the three
      readers look for (`is_human_input_line`, `doing_line`, `row_is_working`)
      rather than a picture of them. The rig and both photographs are under
      `~/Work/reports/live-turn-2026-09-21/`.

## What the photograph caught

**The card was pinned to the floor of the pane, under an acre of empty** — and
the comment at the line that decided it had already been written about exactly
that failure, for the stand-in reply, a month earlier. `body_anchor` and the
body's scroll are both asked *is a card in the room*, and the value answering
was `showing_id`, which this feature deliberately makes `None` while a turn
stands. A test could not have seen it: a layout is not a position a headless
suite can assert about. What holds it now is `room_id` plus a source guard,
`benchdraw::tests::the_room_decides_the_anchor_not_the_surface_in_it`.

The lesson generalises and is the one worth keeping: **a comment explaining why
a condition is what it is, is the test for whatever you add beside it.** The old
sentence said *the stand-in IS a card*. A third kind of card walked straight
past it.

## Difficulty, after the fact

**5 was right, and the ceremony it bought was right too.** One plan page, no
gates, built in one sitting. What the five bought that a three would not have is
the amber list — five decisions taken without asking, each of which could
reasonably have gone the other way, written down where Parker can argue with
any of them rather than discovered later in the code.

Eighteen mutations, eighteen caught, on a harness proven to have all three legs
(a mutation that survives, one that is caught, one that fails to build). One of
them earned its place the honest way: `M16 the rail row calls itself STANDS NOW
again` **survived** with the suite green, which is how the lane words came out
of the renderer into `benchdraw::lane_word` and got a table test.

## What review is for, demonstrated twice

Neither of the two real defects in this feature was found by a test, and both
were found by asking a question no test knows how to ask.

| found by | defect |
|---|---|
| looking at a photograph | the card pinned to the floor under an acre of empty — a proxy predicate that went stale when a third kind of card arrived |
| reading my own words back after approval | `NOTHING PRESENTED` — a claim about the future, drawn in a race that runs on every turn |

Both are the same shape as the bug the feature exists to fix: a surface stating
something confidently in a gap where it does not have it. Having written the
rule down did not stop me writing the violation eleven lines below it.
