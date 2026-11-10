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
```

TLC 1.7.4, one worker, under a second. Each rule is checked on its own and compared with `findings.txt`:

| Result | Meaning | Do |
|---|---|---|
| `holds` | kept | nothing |
| `breaks (finding)` | a known finding, listed in `findings.txt` | nothing; it is reported below |
| `BREAKS` | a new break | read the path (below), then decide: the code regressed, or the model no longer matches the code |
| `FIXED?` | a finding now holds | confirm the code changed to fix it, then remove the line from `findings.txt` |

To read a break as a path in plain words, and to see the whole chart, run the model through the pstack `/statechart` skill with `view.ts` in this folder as its view.

## What it found (2026-11-10, sync commit below)

- **SleepLosesNothing breaks** (issue 1176): a `/loop` waiting on its own wakeup reports itself idle, rests, and sleep ends it, losing the loop. Path: you type to it → it ends a turn with a timer → the window looks: idle → the window freezes it → the window ends it.
- **NeverFrozenMidTurn breaks** (new): the timer can fire between the look and the freeze. `Gate::freeze_if_quiet` refuses only when the *person* wrote since the look, so an agent that started a turn on its own in that gap is frozen mid-turn, and a frozen agent prints nothing, so nothing notices. Path: you type to it → it ends a turn with a timer → the window looks: idle → its timer fires → the window freezes it.

## Coverage

| Model action | Code |
|---|---|
| `Look` | `agentrest_io.rs`, the per-pane look and `Stage::Awake` arm; `agentrest::may_rest` |
| `Freeze` | `panegroup.rs`, `Gate::freeze_if_quiet(group, epoch)` |
| `Sleep` | `agentrest_io.rs`, `Stage::Resting` arm: `may_sleep`, `conversation_saved`, `sleep` |
| `PersonTypes` | the gate's write path (thaw), the wake script (`agentrest::wake_argv`) |
| agent steps | Claude's own behaviour, as measured in issue 1176 |

**Sync commit:** `62f87a5b` (explore/t3-scuttle). Before trusting a result, run `git diff 62f87a5b -- app/src/agentrest.rs app/src/agentrest_io.rs app/src/panegroup.rs`. If the covered code changed, check that each action above still describes it, update the model if not, and only then move the sync commit forward.

**Left out:** time (a threshold is met or not, as one step), memory, Codex, SleepNow, the stop deadline and `Stopping`, more than one pane.

## It can fail

Each rule that holds was broken on purpose and caught (`verify/mutate.sh`):

- Freeze ignoring the person's key (`stage' = "resting"` unconditionally) → `NoFreezeOverATouch` BREAKS.
- Sleep without `conversation_saved` (drop `/\ saved` from `Sleep`) → `SleepKeepsTheConversation` BREAKS.

## When you change the code on purpose

Change the action that stands for the code you changed, run the check, and read any new break as a path before deciding it is wrong. Never weaken a rule to make it pass. A fix for a finding should turn its line into `FIXED?`; remove the line then.
