# End an agent from the bench, and get the bench back

**Difficulty: 5/10** — one combined plan page, one approval.

Half of this is a three-condition boolean that is wrong in two of its
conditions; that half is a 2. The other half puts a destructive verb on a
person's live agent, and the obvious way to build it (`SIGKILL`) silently
defeats the first half. That is what buys the page.

**Status:** BUILT through rung one, in `6e91c33`. Slices 1–5 shipped alongside
[the agent-strip design](../agent-strip/00-status.md), because both features
land in the same bar and building them apart would have meant designing that bar
twice.

Rung two — `Request::KillForeground`, for an agent too wedged to read its own
input — is **not built**. It is the only piece here that needs the host
upgraded, and a wedged agent is the rarer case; rung one covers ending an agent
that is merely running.

Slice 2 ended up stricter than this page proposed. The offer is keyed on
`!agent_now && !wb_had_agent`, which means a pane that has had an agent does
**not** get the body's standalone button at all — its strip carries `LAUNCH
AGENT` instead. One slot, two states, rather than two buttons in two places
offering the same thing.

---

## What Parker asked for

> When I start a new agent in the workbench and then end that agent session, I
> do not have the ability to start another agent from the same workbench. We
> need a kill agent at the top status bar button that will end that agent
> session abruptly. And if we do end an agent session from the workbench, then
> we want to be able to start a new session from the workbench as well.

Two verbs, one slot: **end the agent that is here**, and **start the next
one**.

---

## Why the bench never offers the launch again

`app/src/pane/bench.rs:1551`:

```rust
let offering =
    self.bench.showing().is_none() && !self.mode.is_agent() && self.bench.is_empty();
```

`offering` is the only thing that draws `LAUNCH AGENT`
(`benchdraw.rs:2197`, reached from `pane/bench.rs:1639`). Two of its three
conditions are wrong for the case he is describing:

- **`self.bench.is_empty()`** is false and stays false. The agent presented
  surfaces while it lived, and nothing clears them when it dies — there is no
  caller anywhere that empties a `Bench` on a mode change. So the pane the
  agent actually worked in is the one pane that can never offer a relaunch,
  and an agent that presented nothing at all still can. That is backwards:
  emptiness is a fact about the *record*, and the offer is a question about
  the *process*.
- **the arm it sits in.** The offer is built inside the `None =>` branch of
  `match self.bench.showing()`, so opening any card from the rail also hides
  it.

There is a third, only on the abrupt path, and it is the one that matters for
the design below. `next_mode` (`app/src/host.rs:1585`) holds a pane at
`Claude`/`Codex` for as long as the **alternate screen** is up:

```rust
if was_agent && !still_agent && on_alt {
    return current.cloned();   // an agent shelling out is still an agent
}
```

That is right, and it is why an agent running `rg` does not rename itself
twice a second. But a process that is `SIGKILL`ed never gets to send the
leave-alt-screen sequence. **Kill the agent with a signal and the pane reads
as an agent forever** — which means the naive build of the button Parker asked
for is the thing that permanently withholds the button he asked for next.

---

## The shape: one slot in the AGENT bar, three states

`benchdraw::title_card` (`benchdraw.rs:1533`) is the strip in his screenshot —
`AGENT ⊙ Idle · 6s`. It already ends with `.child(div().flex_1())` at line
1665, a spacer whose only job is to push a trailing element to the right edge.
That spacer is the slot; nothing has to be invented to hold it.

The strip today has two states, and the pane has three:

```
 live agent        AGENT ⊙ Working · 2m   turn 1m  ↓ 4.1k tokens        [ END ]
 agent ended       AGENT ○ Ended · 40s    claude · exit 0        [ ⌁ LAUNCH AGENT ]
 never an agent    (no strip at all — the body's standalone button, as today)
```

Row two does not exist. `let live = self.mode.is_agent().then(…)`
(`pane/bench.rs:1503`) draws the strip only while an agent is live, and
`shows.composer = is_agent` takes the composer away at the same instant — so
the moment an agent exits, every affordance on the bench vanishes at once and
the surfaces it made are left with no chrome around them. Adding the ended
state is what makes the slot able to carry both verbs, and it is also just the
honest reading: *an agent was here and is gone* is not the same fact as
*nobody has ever run one here*, and the code currently stores both as `Shell`.

`set_mode` (`app/src/pane.rs:2783`) is already the single funnel for this and
already acts on one direction of the transition:

```rust
let arrived = !self.mode.is_agent() && mode.is_agent();
```

The departure — `was_agent && !mode.is_agent()` — has no handler. It is one
line beside an existing one, and it is where the ended state gets recorded.

---

## How the kill works

Two rungs. The button is rung one; rung two is what it becomes when rung one
did not take.

**Rung 1 — ask it to quit.** Two `0x03` bytes through `bench_deliver`
(`pane/bench.rs:1251`), the same path the composer already writes keystrokes
on. This is what Parker's own hands do, so it is exactly as abrupt as he
means and no more:

- no host change, so it works against the host running on this machine right
  now;
- the agent leaves the alternate screen on its way out, so `next_mode`
  demotes the pane for real and the ended state is reached honestly;
- the transcript is flushed, which a signal does not do.

It fails on a wedged agent, which is the entire reason for rung two.

**Rung 2 — signal the group.** If the pane still reads as an agent ~2s later,
the button relabels itself `FORCE` and the second press sends `SIGTERM` then
`SIGKILL` to `-pgid`, where `pgid` is `tcgetpgrp` on the pane's
pseudoterminal master — a read the host already performs every watcher tick in
`classify_foreground` (`host.rs:1540`).

This one needs the host, and the host is the constraint:
`Request::ClosePane` is documented as *"the only thing that kills"* and hangs
up the **whole process tree** (`host.rs:943`, `kill(-shell_pid, SIGHUP)`) —
that ends the pane, not the agent in it, so it is not this verb. A new
`Request::KillForeground { pane }` is needed, and it must go in **without**
touching `PROTO_VERSION` (which is `1`, and `version_check` refuses any
mismatch outright rather than negotiating) — so an old host answers the
unknown verb with an error reply, and the window falls back to telling the
person the host predates the verb rather than appearing to do nothing.

Because of the alt-screen trap, rung 2 must **also** force the demotion on the
window side once the signal lands. The kill is the only case where the pane
mode cannot be trusted to correct itself, and it is the one case where we know
for certain the agent is gone, because we are the ones who killed it.

---

## What gets built

| # | Change | File |
|---|---|---|
| 1 | `Ended` bench state, recorded on the agent→not-agent edge in `set_mode` | `pane.rs:2783` |
| 2 | `offering` keyed on "no agent in this pane", not on "the bench is clean"; lifted out of the `showing() == None` arm | `pane/bench.rs:1551` |
| 3 | `title_card` draws in the ended state and takes a trailing verb in the `flex_1` slot | `benchdraw.rs:1533` |
| 4 | `Hit::EndAgent`, beside `Hit::Launch` at `pane/bench.rs:106` | `workbench.rs:658` |
| 5 | Rung 1: two `0x03` through `bench_deliver` | `pane/bench.rs` |
| 6 | Rung 2: `Request::KillForeground`, no `PROTO_VERSION` bump, forced demotion on success | `hostproto.rs`, `host.rs`, `hostctl.rs` |

Tracer slice is 1–5: it is the whole gesture, it needs no host change, and it
can be driven and photographed on the running build. 6 lands behind it.

## Done when

- An agent is launched from the bench, ended with the new button, and the
  **same pane** offers `LAUNCH AGENT` without being closed or reopened.
- The same is true when the agent was ended by typing `/exit` in the terminal
  rather than by the button — the offer follows the process, not the gesture.
- A pane that never held an agent is unchanged.
- The surfaces the ended agent made are still readable; the bench does not
  clear them to earn the offer.

## Open, and deliberately not decided here

- Whether the ended agent's surfaces should be visibly marked as belonging to
  a finished session. They are a true record either way, but the rail gives no
  sign that the agent behind them is gone. Related: **#510**, an agent that
  died within seconds and the tab said nothing.
- Whether `ctl bench end` should exist as a scriptable twin, which is what
  **#492** asks for on the launcher panel.
