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
- [ ] looked at on a screen

## Difficulty, after the fact

_(filled in at the end)_
