# Persistence requirements: the workbench's history survives what a pane does not

Parker, 2026-09-21, after the merge: *"specific case now to the persistence of
workbench history: will the overview of turns persist on shutdown/restart of a
full TD instance, and of a session process ending and restarting — a tab /
project closed and reopened inside TD; specific turn contents INCLUDING THE
PERSON'S exact prompt; questions answered, artifacts, same scenarios. These
should be requirements."*

They are. This page states them as acceptance tests, then records what the
build at `fc856b9` does today — **measured, in a demo session, not reasoned** —
and what closes the gap.

## The requirements

Each is a Given/When/Then a test can run, and each is a promise the product
makes regardless of how the record happens to be stored.

**R1 — A window restart loses nothing.**
Given a pane whose bench holds turns, answered questions and artifacts,
when the window is closed and a window is opened on the same session,
then the same bench shows the same turns, answers and artifacts, each with
the same provenance word, and the person's exact prompt over each reply.

**R2 — A full instance restart loses nothing.**
Given the same pane, when the host and the window both exit and the
session is opened again, then the same bench shows the same record — and it
does so because the record is keyed by the *conversation*, not because the
restore happened to number the pane the same way.

**R3 — Reopening the conversation anywhere finds its record.**
Given a conversation that has a record, when that conversation is resumed
in a new pane, a new tab, a new window or a new session, then its bench
shows its record from before, and a bench for a *different* conversation
shows nothing of it.

**R4 — Every reply carries the person's exact words.**
Given the harness reported the prompt of a turn, when that turn's reply is
drawn — the newest or any older one — then the prompt above it is the one the
person typed, verbatim, and is marked as exact; when the harness did not
report it, the caption says the words are the screen's reading, never
nothing and never a guess.

**R5 — An answered question stays answered, with what was chosen.**
Given a round answered on the bench or in the terminal, when the record is
shown again under R1–R3, then every question in it reads as answered with
the labels chosen, and is not offered for answering again.

**R6 — Artifacts keep their location and their writer.**
Given a surface presented over MCP, by file or by the channel, when the
record is shown again under R1–R3, then the surface is there with the same
origin word — and a surface that arrived with no known writer still says so.

**R7 — Nothing pre-existing is migrated, and the reader says so.**
A record written before the conversation key existed is shown as *from a
stale version* rather than folded in wrongly (Parker's decision on the
tenancy brief: *"we do not want garbage code that is taking out-of-date
agents/panes and then maintaining that code"*).

## What the build at `fc856b9` does today — measured

Session `decouple-demo2`, build `td-f7de467-decouple`, a two-question round
answered through the channel, then three restarts. Photographs in the
session's scratchpad (`persist-A.png`, `persist-B.png`, `persist-C.png`).

| Case | What was done | Overview of turns | Exact prompt | Answered questions | Artifacts | Verdict |
|---|---|---|---|---|---|---|
| **A · window restart** (host alive) | `kill <window>`; relaunch with the same `TD_SESSION` | back — the reply card and the overview's *stands now* | back, verbatim, as the caption over the newest reply | back, answered (the journal replays `question` then `answered`) | back — `*.json` files are re-read on open | **meets R1**, by the mailbox being re-read and the inbound journal being replayed from byte 0 |
| **B · full restart** (host and window) | `kill <window>`; `kill <host>`; relaunch | back | back | back | back | **passes by coincidence, fails R2's clause.** The host numbers panes from 1 at every start (`host.rs`, `next_pane: AtomicU64::new(1)`); the restore recreated the two tabs in saved order, so the agent was pane 2 again and found pane 2's mailbox. Close one tab before the restart and every later pane's history lands on the wrong bench. |
| **C · reopen in a new pane** | `ctl adopt --run "claude --resume <same session>"` in the running instance | **gone** — *No responses yet* | **gone** | **gone** | **gone** | **fails R3.** The new pane got id 4; its mailbox is `surfaces/<session>/4/`, empty. The record is intact under `…/2/` and nothing keys it to the conversation. |
| **per-turn prompt** | any case | — | **only the newest**: the *YOU* caption is one latched string (`wb_asked`), drawn over the standing reply; older reply cards carry no prompt | — | — | **fails R4** for every reply but the last |

What already holds, and is worth keeping as the foundation: the person's
exact prompt IS on disk from the moment it is typed (`inbound.jsonl`,
`type: prompt`, from the harness's own hook, since #624), so R4's *words*
exist for every turn — they are simply not attached to each reply's card.
And the whole channel record replays idempotently (test
`replaying_the_journal_leaves_the_state_where_it_was`), which is what makes A
and B come back.

## What closes the gaps

Three pieces, in dependency order. The first is not this feature's to build.

1. **Key the record by the conversation, not the pane** — R2, R3.
   This is `docs/plans/workbench-follows-the-agent/` (the tenancy pane's
   design, approved at Gate 1 and 2): a `conversations/<root>/<seq>/` store
   the *window* files into from the pane mailbox, keyed by the agent's ledger
   root with `paneident` as the fallback, `Bench::set_conversation` on the
   agent-arrived edge, and no migration (R7). The channel's journals fit it
   without a wire change: every inbound record already carries `session_id`
   and `pid`; filing them under the root is the same move as filing a
   surface. The follow-up that carries the channel side is #629.
2. **Attach each turn's prompt to its reply** — R4.
   `turns.jsonl` in that store gives every reply the ordinal of the ask it
   answered; the channel's `prompt` record is the exact text for that ordinal,
   and `Origin::Hook` on the record is what lets the caption say *exact* while
   a scrollback latch says *as the screen showed them* (#633). Until the store
   lands, the cheap half is a per-pane list of `(prompt, at_ms)` in
   `channel::State` and a lookup by arrival time when a `response` surface is
   presented — an inferred join, drawn as such.
3. **Stop depending on pane numbering in the restore** — R2's clause.
   Once (1) exists the numbering coincidence is harmless. Until then it is a
   live hazard, and the honest thing the restore can do is nothing clever:
   R2 is met by (1), not by making pane ids stable.

## Acceptance, as tests that fail today

- `a_conversation_resumed_in_a_new_pane_shows_its_record` — build the record
  under one pane id, present the same conversation under another, assert the
  bench loads the record. Fails today: C.
- `a_reply_that_is_not_the_newest_still_carries_its_own_prompt` — three turns,
  open the first reply's card, assert its caption is the first prompt. Fails
  today: R4.
- `a_restart_that_renumbers_panes_does_not_move_history_between_benches` —
  two conversations, restore them in the opposite order, assert each bench
  shows its own. Fails today: B's clause.
- The A case is already held by `replaying_the_journal_leaves_the_state_where_it_was`
  and the mailbox re-read; keep it.

## Not requirements, said so

- The bench's *view* state — which shelf was open, which card, the fold of a
  register, the draft in the composer — is not part of the record and is not
  promised across a restart. A draft that survives a restart is a separate
  ask.
- Two windows on one conversation showing the same record is R3's natural
  consequence and is fine; two windows *answering* one question is #631.
