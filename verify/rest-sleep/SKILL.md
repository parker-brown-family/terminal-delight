---
name: verify-rest-sleep
description: "Model-check Terminal Delight's agent rest and sleep (freezing an idle agent, ending a rested one and keeping its conversation) with TLA+. Use before and after changing app/src/agentrest.rs, app/src/agentrest_io.rs, or Gate::freeze_if_quiet in app/src/panegroup.rs, or when asked whether rest can freeze a busy agent or sleep can lose work."
---

# Verify rest and sleep

**The promise.** Rest never freezes an agent in the middle of a turn, sleep never ends an agent that still has work pending, a sleeper always has a conversation to wake into, and rest never freezes a pane the person just wrote to.

**The harm if it breaks.** An agent working unattended is frozen and stays frozen until someone touches its pane, or a self-paced `/loop` is ended and its next turn never comes.

## Run it

```bash
verify/check.sh verify/rest-sleep/RestSleep.tla
verify/check.sh verify/rest-sleep/RestSleep.tla verify/rest-sleep/RestSleep-before.cfg
```
```bash
cd app && cargo test --locked --bin terminal-delight agentrest
```

The first checks the code as it is (`Fixed = TRUE`): every rule holds and `findings.txt` is empty. The second checks the code before 2026-11-10 (`Fixed = FALSE`) and must still read `breaks (finding)` for `NeverFrozenMidTurn` and `SleepLosesNothing`; it keeps the findings reproducible. The tests are the real code: `a_scheduled_wakeup_is_pending_until_its_time_and_only_then`, `a_claude_waiting_for_its_own_wakeup_neither_rests_nor_sleeps`, `a_freeze_stands_only_if_the_agent_says_what_it_said_at_the_look`, and `the_end_of_a_conversation_is_read_in_whole_lines_and_finds_the_last_wakeup` in `agentrest_io.rs`.

TLC 1.7.4, one worker, under a second. Each rule is checked on its own and compared with `findings.txt`:

| Result | Meaning | Do |
|---|---|---|
| `holds` | kept | nothing |
| `breaks (finding)` | a known finding, listed in `findings.txt` | nothing; it is reported below |
| `BREAKS` | a new break | read the path (below), then decide: the code regressed, or the model no longer matches the code |
| `FIXED?` | a finding now holds | confirm the code changed to fix it, then remove the line from `findings.txt` |

To read a break as a path in plain words, and to see the whole chart, run the model through the pstack `/statechart` skill with `view.ts` in this folder as its view.

## What it found (2026-11-10), and what fixed it

- **SleepLosesNothing broke** (issue 1176): a `/loop` waiting on its own wakeup reports itself idle, rests, and sleep ends it, losing the loop. Path: you type to it → it ends a turn with a timer → the window looks: idle → the window freezes it → the window ends it.
- **NeverFrozenMidTurn broke** (new): the timer can fire between the look and the freeze. `Gate::freeze_if_quiet` refuses only when the *person* wrote since the look, so an agent that started a turn on its own in that gap is frozen mid-turn, and a frozen agent prints nothing, so nothing notices. Path: you type to it → it ends a turn with a timer → the window looks: idle → its timer fires → the window freezes it.
- **The fix, two parts, each shown to hold its rule alone** (see "It can fail"): the look reads the end of the conversation file and refuses a `ScheduleWakeup` still to come (`agentrest::wakeup_pending`, `Refusal::WakeupPending`; a waiting `/loop` does not rest at all), which closes the timer path for both rules; and a freeze stands only if the agent, frozen, says what it said at the look (`agentrest::said_the_same`; otherwise `Refusal::MovedSinceTheLook` and the gate releases it at once), which holds `NeverFrozenMidTurn` for any self-start that says so before the freeze.

## Coverage

| Model action | Code |
|---|---|
| `Look` | `agentrest_io.rs`, the per-pane look and `Stage::Awake` arm; `agentrest::may_rest` |
| `Look` (Fixed) | also `agentrest_io.rs` `conversation_tail` → `agentrest::wakeup_pending`, and `may_rest`'s `wakeup_pending` arm |
| `Freeze` | `panegroup.rs`, `Gate::freeze_if_quiet(group, epoch)` |
| `Verify` (Fixed) | `agentrest_io.rs` `run`, Stage::Awake arm: `agentrest::said_the_same(look, frozen)`, else `Gate::release` |
| `Sleep` | `agentrest_io.rs`, `Stage::Resting` arm: `may_sleep`, `conversation_saved`, `sleep` |
| `PersonTypes` | the gate's write path (thaw), the wake script (`agentrest::wake_argv`) |
| agent steps | Claude's own behaviour, as measured in issue 1176 |

**Sync commit:** the commit that last changed this skill (`git log -1 --format=%h -- verify/rest-sleep/SKILL.md`). Before trusting a result, run `git diff <that commit> -- app/src/agentrest.rs app/src/agentrest_io.rs app/src/panegroup.rs`. If the covered code changed, check that each action above still describes it, update the model if not, and only then move the sync commit forward.

**Left out:** time (a threshold is met or not, as one step), memory, Codex, SleepNow, the stop deadline and `Stopping`, more than one pane. Also a turn an idle Claude starts on its own from its prompt with no timer (a Monitor event, a session cron): if it is frozen in the moments before it writes `busy`, the second reading still says idle and the freeze stands. The window between the look and the freeze is one background tick, and `CronCreate` crons are not read at all.

## It can fail

Each rule that holds was broken on purpose and caught (`verify/mutate.sh`):

- Freeze ignoring the person's key (`stage' = "resting"` unconditionally) → `NoFreezeOverATouch` BREAKS.
- Sleep without `conversation_saved` (drop `/\ saved` from `Sleep`) → `SleepKeepsTheConversation` BREAKS.
- The look no longer refusing a pending wakeup (`agent \in {"prompt", "waiting"}` under Fixed) → `SleepLosesNothing` BREAKS, while `NeverFrozenMidTurn` still holds, by the second reading alone. Also making the freeze stand without that reading (`Verify` always rests) → `NeverFrozenMidTurn` BREAKS.
- In the real code, `may_rest` letting `Some(true)` through fails `a_claude_waiting_for_its_own_wakeup_neither_rests_nor_sleeps`, and `said_the_same` ignoring `since_ms` fails `a_freeze_stands_only_if_the_agent_says_what_it_said_at_the_look`.

## When you change the code on purpose

Change the action that stands for the code you changed, run the check, and read any new break as a path before deciding it is wrong. Never weaken a rule to make it pass. A fix for a finding should turn its line into `FIXED?`; remove the line then.
