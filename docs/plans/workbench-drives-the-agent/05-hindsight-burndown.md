# Hindsight burndown: the agent channel, one day in

Written 2026-09-21 in the afternoon, after the first pass merged (#624 at
`be60bf2`) and the hardening pass was built. Parker asked for *"a hindsight
burndown — code quality and smell pass retro, anything you WISH you would have
done on the first pass… any little future proofings for this eventually getting
into a mobile app for mobile control of the workbench… then automated tests to
keep everything buttoned up, hunting down edge cases."* This is that, in five
parts: what the day measured against what the morning assumed; the smells, each
with what was done about it; the edge cases hunted and the tests that now hold
them; the seams a mobile client needs; and the follow-ups filed as issues.

## 1 · What the afternoon measured that the morning assumed

| Morning's claim | Afternoon's measurement | Consequence |
|---|---|---|
| A hook can pre-answer a question (driven once, under a pty, haiku) | **Also on a real window, a real opus agent, twice.** Round two: file road, `You chose Autumn`, no picker. Round three: multi-select, `ANSWERS="Red, Blue"`. | The design's keystone is measured on the product path, not only in a harness. |
| The keys road is the fallback for an older window or no bench | **It fired on round one for a reason nobody predicted**: the marker read as stale when the pane's face or tab changed mid-wait, the picker painted, the screen reader merged its cursor onto the hook card, and the press drove the picker by keys — journaled as such. | The fallback works. The record could not say *why* it fell back — `stale` covered both a dead window and a person flipping a face. Fixed: `released.why` now distinguishes `closed`, `stale`, `missing`, `timeout`, `answered`, and carries `at_ms`. |
| A multi-select answer is the labels joined by `", "` (inferred) | **Measured**: the file `{"Which colours do you like?": "Red, Blue"}` came back through `PostToolUse` unchanged. | The doubt is closed in the spec and the docs. |
| `waiting_question()` picks the newest arrival | **Wrong for a round that arrives whole**: the newest arrival is the round's LAST question, so a two-question round opened on the second. Found by the pane building the navigator, reproduced from the other side. | Theirs to fix, and being fixed; it is the exact jam Parker photographed in #619, reached by a new road. |
| `bench.json` is a window-owned file in the mailbox | **Every `.json` in the mailbox is a surface to the sweep.** Read before wiring the beacon: it would have put an `unclassified` card on every bench at one hertz. | Skipped by name, with a test that sweeps a directory holding one. The lesson is a memory note: window-owned state in the mailbox needs a non-`.json` name or a subdirectory. |
| A launch under `TD_SCRATCH=1` is the safe demo window | **A scratch window never hosts**, so its panes carry no `TD_SESSION`/`TD_PANE_ID` and the channel has nowhere to write. | The demo needed a hosted window under a fresh key (`TD_SESSION=decouple-demo`). Written into the demo notes. |

## 2 · Smells, and what was done about each

**Fixed in the hardening pass**

1. **`released: stale` said one word for two events.** A dead window and a
   flipped face read the same. Now five reasons, each a fact a reader can act
   on, each stamped.
2. **The marker was written once a second per agent pane whether or not a
   bench was open.** Twenty panes, twenty writes a second, for a fact that only
   changes when a person flips a face. `channel::beacon_due` is a pure rule
   with a table test: an open bench refreshes every second; a closed one writes
   on the change and never again. The hook reads the face before the clock, so
   a closed marker does not need to be fresh.
3. **A hook-carried multi-select could be ticked from the control socket but
   never submitted.** `bench_choose` ran every card through `nav_index`, which
   is right for a picker (its Submit sits between the options) and wrong for a
   channel card (its Submit sits after them by construction). `ctl bench choose
   4` on a three-option multi was refused as *no option 5*. Channel cards skip
   `nav_index` now.
4. **An `answered` arriving before its `question`** — a journal replayed from
   an offset, two hooks racing — presented a settled question as live. The
   state remembers such answers and closes the question on arrival.
5. **The rounds cap evicted the oldest round, open or not.** It evicts the
   oldest *settled* round first, and an open one only when all sixteen are open.
6. **Hook reply ids were the clock alone.** Two replies in one millisecond were
   one surface. A per-pane counter rides beside the clock.
7. **Editing a recalled message did not end history mode**, so the next arrow
   threw the edit away. An edit ends the recall.
8. **Nothing on an outbound line said which mailbox it was written in.** A
   reader multiplexing panes had to remember the path. `session` and `pane`
   travel on every line now (TDAC 0.2).
9. **The inbound journal grew without bound.** Rotation at 2 MiB, under the
   same lock as the appends; the window's offset reader treats a shrink as a
   fresh start and every record is idempotent to re-present.
10. **The hook re-read the marker four times a second**, two `jq` spawns per
    read while waiting. Once a second now; the answer file is still polled at
    250 ms.

**Named and left, on purpose**

11. **A multi-select answer is drawn as `Answered::Typed("Red, Blue")`.** The
    enum has `Chose(usize)` for one option and `Typed(String)` for prose, and a
    set of options is neither. The honest shape is a `ChoseMany(Vec<usize>)`
    variant, and it touches the renderer — the navigator pane's ground today.
    Filed.
12. **`channel::State` lives on `TerminalView`, not on `Bench`.** Right for
    now (the tenancy pane is re-keying `Bench`), and it means the ownership
    move has to carry the channel state with the bench when it happens. Filed
    with the move.
13. **The composer's copy is the whole draft.** Select-all is the only
    selection the composer has; a drag-selection inside the box is the
    text-selection pane's shape to extend. Not filed — it is on their list.
14. **Two windows on one session both write one pane's marker.** They agree
    on the face only if they show the same tab. Rare; the hook takes whichever
    wrote last. Filed as an edge to decide, not a bug to fix blind.
15. **`file_key` folds punctuation**, so two tool-use ids differing only in
    punctuation name one answer file. Real ids are alphanumeric; noted in the
    same issue as 14.

## 3 · Edge cases hunted, and the tests that hold them

Twenty channel tests (was thirteen), twelve adapter tests (was ten), plus the
feed's two. Each new one names the case it holds:

| Case | Test |
|---|---|
| answered before asked | `an_answer_that_arrives_before_its_question_closes_the_round_on_arrival` |
| the cap under open rounds | `the_cap_evicts_a_settled_round_before_an_open_one` |
| a window restart replaying the journal, and a cold reader seeing a dead `waiting` | `replaying_the_journal_leaves_the_state_where_it_was` |
| two identical questions in one round | `two_questions_with_one_text_in_a_round_are_matched_in_order` |
| the marker's write schedule, open and closed | `the_beacon_refreshes_an_open_bench_and_writes_a_closed_one_once` |
| two replies in one millisecond | `hook_replies_get_distinct_ids_inside_one_millisecond` |
| the mailbox names the line | `an_outbound_record_carries_its_road_and_the_keys_road_is_never_silent` (extended) |
| **the real adapter, driven and read end to end** — bash and jq under a spawned process, the reader tailing the journal, a press writing the answer file, the pre-answer on stdout, the release read back | `a_question_round_trips_through_the_real_adapter_and_the_reader` |
| a face flipped mid-wait releases as `closed`, not `stale` | adapter: *a bench that closes mid-wait releases the picker as closed, not stale* |
| the journal rotates under the lock | adapter: *the journal rotates at the cap, under the same lock the appends take* |
| no marker at all is `missing`; a marker from the future is `stale` | adapter: the two-marker case, retargeted |

What is still not tested, and said so: the gpui key path (the harness runs
without a window, #497/#586); Codex and Gemini; two windows on one session.

## 4 · What a mobile client needs, and where the seams already are

Parker: *"any little future proofings for this eventually getting into a mobile
app for mobile control of the workbench."* The channel was built as files in a
directory, and a phone cannot read this machine's disk. The seams that make a
relay cheap are these, and the first three exist today:

1. **Every record names its mailbox.** Inbound lines carry `pid` and
   `session_id`; outbound lines now carry `session` and `pane`. A relay that
   tails every pane's journals can stream one multiplexed feed and route a
   phone's press back to the right `answers/` directory without inventing a
   join key.
2. **The state machine has no window in it.** `channel::State` is values in,
   effects out — no gpui, no pane, no filesystem. A relay process can host one
   per pane and expose `press`/`say` over a socket; the window and the phone
   would then be two clients of the same state rather than two implementations
   of it. That is the single most valuable property to protect, and the
   round-trip test is the proof it holds.
3. **The answer is a file any client may write.** The hook does not care who
   wrote `answers/<id>.json`. A relay writing it on a phone's behalf is exactly
   the window writing it on a click.
4. **What does not exist yet — a transport.** The natural one is Terminal
   Delight's own MCP server, which already exposes `list_panes` and
   `present_surface` and already refuses writes unless a toggle is on: add
   `channel_events` (a tail with a cursor), `channel_say` and `channel_press`,
   and a phone app becomes an MCP client over a tunnel. The writes toggle is
   the consent gate, and it is already there.
5. **What must be decided before that**, and is filed: identity (which device
   pressed, drawn on the card the way `Origin` is drawn now); a second person
   (#484 already names it — every single-user assumption on the bench); and a
   phone that presses while the desktop's bench is also open (the marker is the
   desktop's claim; a relay would need its own, or the hook would need to
   accept either).

None of this was built today. Numbers 1–3 were shaped so that it can be.

## 5 · Follow-ups filed

Each as a falsifiable GitHub issue, labelled `follow-up`, with the check that
would prove it wrong to run first:

- `ChoseMany` for a multi-select answer (smell 11).
- Move `Bench` and `channel::State` off the pane together, after the tenancy
  key lands (smell 12; frame D in the brief).
- Codex and Gemini adapters, each driven before it is trusted.
- Two windows on one session share one marker; `file_key` folds punctuation
  (smells 14 and 15).
- A relay for another device, as a design brief before code (part 4).
- The overview caption and the reply card say which record they came from
  (slice 9).

## What I would do differently on a second first pass

Read the sweep before naming a file in the mailbox. Say `closed` and `stale`
from the first cut — the one-word reason cost a live round its explanation.
Give every record a clock from the start, not only the ones I thought would
need ordering. And test the round trip through the real script on day one: it
took twenty minutes to write, it exercises the bash, the reader and the press
in one motion, and it is the test that would have caught the next drift between
the two sides of a file-based protocol.
