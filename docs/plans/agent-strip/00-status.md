# The AGENT strip becomes the agent's control bar

**Difficulty: 4/10** — one combined plan page, and the design choice is already
made rather than offered. Parker: *"You have the power to make a good design
choice here."*

**Status:** decided, building. Branch `bench/kill-and-relaunch`.

---

## What it is now, and what goes in it

The strip in `benchdraw::title_card` (`benchdraw.rs:1533`) reads state and
nothing else, and it ends with a `flex_1()` spacer at line 1665 whose only job
is to push a trailing element to the right edge. That spacer is a slot nobody
has filled, on a bar that is otherwise empty across most of its width.

Everything asked for in this session goes in that trailing run, in one order:

```
AGENT ⊙ Working · 2m   turn 1m  ↓ 4.1k  Read        opus ▾   high ▾    ⏹ END
└────────── state, unchanged ─────────┘             └── dials ──┘   └─ verb ─┘

AGENT ○ Ended · 40s    claude · exit 0                        ⌁ LAUNCH AGENT
└──── the third state the pane has always had ────┘           └── the way back ─┘
```

The ended row is the other half of
[the kill-and-relaunch plan](../bench-kill-and-relaunch/00-status.md); the two
features share this bar and are built against one another on purpose.

---

## Three claims, and each is a rule rather than a look

### 1. A dial says what the agent was TOLD. It never guesses.

Nothing can read the model out of a running agent. The strip knows exactly two
things: what the launcher started it with — `--model` / `--effort` on the
command line, which `parse_model` (`main.rs:23601`) already reads back — and
what the strip has itself set since. Everything else is **unknown**, and an
unknown dial reads `model ?`, not `opus`.

This is not fastidiousness. Parker starts agents by hand in terminals
constantly, and a bar that showed `opus · high` on one of those would be
inventing a fact about billing and about how hard the thing is thinking. A
dial nobody has set and a dial set to the default are different states; the
machine-global rule is that absence is modelled in the type, and here it is an
`Option` all the way to the paint.

### 2. Picking a value types the harness's own slash command.

Verified against the installed `claude` (2.1.274, the binary at
`~/.local/share/mise/installs/claude/2.1.274/claude`): both commands exist and
both take an inline argument. The bundle carries the literals `"/model,
/effort"`, `"run /effort high to continue"`, `"try /effort medium"`, and
`argumentHint:"[model]"` — and its own help text says *"`/effort` controls how
long Claude thinks before answering"*.

So the whole mechanism is one call:

```rust
self.bench_say("/effort high", cx);
```

`bench_say` (`pane/bench.rs:1236`) already exists, already writes down the
pseudoterminal, already holds the write when nobody is looking at the pane. No
host verb, no wire change, nothing that needs the running host upgraded.

**The values come from `launcher.rs` and are not re-listed here.**
`Harness::models()` and `Harness::efforts()` are already the harness's own
lists, already checked against `claude --help` on 2026-09-17, already clamped
per harness by `clamp_effort` so a level Codex does not take is never offered
for Codex. A second copy of that list is how a menu goes stale and silently
starts the wrong model.

### 3. The dials go grey while the agent is working.

A slash command typed mid-turn does not take effect mid-turn — it queues in the
harness's own line editor and fires whenever the turn ends, which is a change
at a moment nobody chose. The strip already knows the state it is in and
already colours by it. `Working` greys both dials; `Idle` and `Waiting on you`
light them.

---

## The card scrolls. The folds stay.

Parker: *"the bottom of this element is cut off. It should be, if not
scrollable, then broken into separate elements that are then collapsible."*

It is already broken into collapsible elements — the fold arrows are in his
screenshot, `benchdraw::response` takes `Folds`, and it did not save him. It
cannot: one unfoldable section — a long evidence list, a patch, a table — can
be taller than the pane on its own. **Folding is a convenience and scrolling is
the correctness fix**, so it is scroll, and the folds stay for the reason they
were added.

Where it is cut off, and why nothing reaches it:

- The body is `div().flex_1().min_h(px(0.)).overflow_hidden()`
  (`pane/bench.rs:1914`). Clipped, with no scroll of any kind.
- An **opened** card is `Anchor::Top` (`body_anchor(card_open = true, _)`), so
  the overflow falls off the **bottom** — which is exactly what he is looking
  at. The stand-in card, which nobody opened, is `Anchor::Bottom` and clips at
  the top instead. Both are unreachable.
- `wheel_target` (`workbench.rs:748`) returns `Composer` over the composer,
  `Mirror` when the mirror is up, and `Nothing` otherwise. The mirror is off
  behind `TD_BENCHMIRROR`, so **over a card the wheel is routed to nothing at
  all** — there is no gesture, no key and no scrollbar that can reach a clipped
  section.

So: a `ScrollHandle` on the card container, `overflow_y_scroll`, and a
`Wheel::Card` arm beside `Wheel::Composer`. The composer already does exactly
this (`benchdraw.rs:1741` and `1938`), so it is the same shape twice rather
than a new mechanism.

**The one trap, and it is already written down:** `justify_end` makes a scroll
container's top unreachable. A scrolling card must stay top-anchored — only the
conversation keeps `justify_end`, and the conversation must not become the
scroll container.

---

## What gets built

| # | Change | File |
|---|---|---|
| 1 | `ScrollHandle` + `overflow_y_scroll` on the card body, top-anchored | `pane/bench.rs:1914` |
| 2 | `Wheel::Card`, and the arm that drives the handle | `workbench.rs:748`, `pane/bench.rs:159` |
| 3 | `Option<Model>` / `Option<Effort>` on the pane, set at launch and on each press, `None` otherwise | `pane.rs` |
| 4 | Two dials in `title_card`'s trailing slot, greyed while Working | `benchdraw.rs:1533` |
| 5 | `Hit::Dial` and its little menu, values from `Harness::models()` / `efforts()` | `workbench.rs:658` |
| 6 | The press: `bench_say("/effort high")` | `pane/bench.rs` |

## Done when

- A response card longer than the pane can be read to its end with the wheel,
  with no shelf change and nothing opened from the rail.
- A test asserts the wheel over a card routes to the card. `wheel_target`'s
  existing tests (`workbench.rs:3535`) cover the composer and the mirror and
  say nothing about a card, which is how this shipped clipped.
- Pressing `high` on an idle Claude pane leaves `/effort high` submitted in the
  terminal and lights `high` on the dial.
- A pane whose agent was started by hand shows `model ?` and `effort ?`, and
  shows them until somebody presses one.

## Open, and deliberately not decided here

- **`max` does not reach a REMOTE session**, and that is the only restriction on
  it. `claude --help` at 2.1.274 confirms all five levels — `--effort <level>
  (low, medium, high, xhigh, max)` — so `Harness::efforts()` is right. The
  refusal string in the bundle turns out to be about cloud sessions rather than
  about models: *"`${e}` is session-scoped and won't reach the remote process.
  Use low, medium, high, or xhigh instead."* This window launches local agents,
  so the two dials are independent and neither needs to clamp the other. Worth
  recording because the bundle carries five different effort scales
  (`["low","medium","high"]`, `…,"immediate"]`, `…,"max"]`, `…,"xhigh"]` and the
  full five) and only the last is the one `--effort` takes — so a future reader
  grepping for an effort list will find four wrong ones first.
- Whether the dials belong on the strip at all on a narrow pane. The strip is
  one line by deliberate decision and it already drops the tool label first;
  the dials need a place in that order.
