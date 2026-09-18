# A launch lands where you are standing

**Difficulty: 6 / 10.** Two behaviour changes in a live surface, one of which
writes a command line into somebody's shell. The blast radius is small and every
decision is testable without a window, which keeps it out of the sevens; the
write, and a placement rule that an existing guard test was already watching,
keep it out of the threes. One combined plan page, this one.

Score submitted before the work; the honest number after it is at the bottom.

## What happened

Parker pressed **LAUNCH AGENT** on a pane's empty bench. A tab appeared at the
bottom of the left bar under UNFILED, a screen away from the branch he was
working in; the pane he pressed the button on was still an empty shell. Later,
a message he sent from a bench arrived nowhere — it started a new Claude session
with the message itself as the argument.

Three findings came out of it, two of them defects in this repository.

### 1. Every launch opened a loose tab at the end of the window — #508

`adopt_pane` built its tab with `Tab::new(node, None)` and pushed it onto the end
of the strip. `None` is the group. Nobody chose it: it is the constructor's
default for a field the launcher never thought about, and `push` is the end of
the window rather than a decision about where the tab belongs.

The `+` key had already solved this. `new_tab_in` seats a tab at
`branch_end(place)` and inherits the group and its project — and its own doc
comment claimed to be *"the only place a new tab is built"*, which stopped being
true when `adopt_pane` grew a second one.

### 2. The bench typed into a pane with no agent — #509

The composer is a **mirror** of the agent's own line editor, not a buffer that is
submitted at the end: every keystroke goes straight down the pseudoterminal and
the echo comes back from the agent. On a pane whose agent had exited, the shell
underneath collected those keystrokes into a command line, and the return key —
`\r`, a keystroke like any other on that path — ran it.

`claude <the whole message>` is a valid command line. That is how a bug report
sent from the bench became a new session with itself as the argument and no
history, while the bench went on drawing the conversation it thought it was
talking to. The scripted twin, `ctl bench say`, had refused this all along:
`v.mode.is_agent()`. The hand path had no such check.

### 3. A launched agent died in seconds and the tab said nothing — #510

Not fixed here, and not explained. The `--effort high` launch of that morning ran
its SessionStart hook at 08:14:27, wrote no transcript, drew no banner, and left
a bare prompt. The launch eighteen seconds later, same directory and same
briefing, is still running. The one mechanism cheap enough to test — the tab
going off screen shrinking the pty — was tested and does not kill a booted
`claude`, so the issue carries that as ruled out rather than as a theory.

What this work does about it: the bench gate turns the silent version of that
failure into a visible one. A pane whose agent has gone no longer looks like a
pane you can talk to.

## What the button already promised

The decision about where a launch should land was not mine to make. LAUNCH AGENT
is drawn on exactly one surface — the empty bench of a pane with no agent — under
a sentence this repository already shipped:

> A shell has no agent to present anything. **Launch one into this pane.**

So the fix is the button keeping its own label, and the only real question is
when it must not: when the pane is busy, and when nobody has read the pane yet.

## What shipped

**One decision function, three states.** `launcher::landing(at_a_prompt:
Option<bool>)`. `Some(true)` — the foreground process group IS the shell, so
anything it started has finished — lands in the pane. `Some(false)` is a pane
with something running in it. `None` is a pane nobody has classified: a
host-owned pane is born `Unknown`, and "we have not looked" must never fall
through to a write. `PaneMode::at_a_prompt` is the mapping, and it is the reason
`foreground_mode` no longer answers `Shell` to a `tcgetpgrp` that failed.

**The typed line clears the prompt first.** `^U`, then `cd` only when the project
is somewhere else, then the recipe, then a newline. The gate says the shell is at
its prompt; it does not say the prompt is empty, and a half-typed line with a
command appended to it runs as one command.

**One tab-building site again.** `open_tab(restore, seat)` — where `Seat` is
`Branch(place)` or `Loose` — and `new_tab_in`, `adopt_pane` and the launcher all
come through it. A launch and a resurrection are seated in the branch you are
standing in; `ctl adopt` stays loose, deliberately: a terminal handed over by
another session belongs to whoever sent it, not to whatever project this window
happens to be looking at.

**One gate on every bench write.** `bench_may_write()` asks both questions — on
screen, and an agent to receive it — and the five write sites go through
`bench_keystroke` (a keystroke) or `bench_deliver` (a line). Held writes wait in
the queue that already existed for off-screen panes, and land when the pane comes
on screen *or* when an agent appears in it, whichever was missing. Typing on the
bench of a pane with no agent no longer opens an invisible composer.

**The header says what it is holding and why.** `3 held · no agent in this pane`
rather than a silent count.

**The panel says where ↵ goes** — `↵ starts in this pane` or `↵ opens a new tab`
— read from the same function the launch reads, so the label and the landing
cannot disagree.

## What holds it

| Test | Holds |
|---|---|
| `only_a_pane_somebody_has_read_and_found_idle_gets_the_agent` | the three states, including that unknown is not a prompt |
| `the_typed_line_clears_the_prompt_first_and_only_travels_when_it_has_to` | `^U`, the conditional `cd`, the newline |
| `a_directory_with_an_apostrophe_reaches_the_shell_whole` | the quoting, against a real `sh` landing in a real directory |
| `a_launch_is_seated_and_an_adoption_is_deliberately_loose` | the placement, the pane landing, and that the hint reads the same function |
| `a_hosted_window_makes_no_pane_of_its_own` | extended to the new single site, with each gesture asserted to delegate |
| `every_bench_write_goes_through_the_gate` | scans `bench.rs` for a write outside the gate, and that the gate still asks both questions |

The bench tests are source scans because a `Workspace` and a `TerminalView` need
a live gpui `Window`, which no test in this crate has. That is the existing
convention here and its limits are written into the tests beside them.

## What is deliberately not here

- **#510 stays open.** A launched agent can still die quietly; this only makes
  the aftermath visible.
- **`foreground_mode`'s other guesses.** One line changed — the kernel declining
  to answer now reads `Unknown` instead of `Shell`, because that is the answer a
  write now depends on. The rest of that reading was not touched.
- **A "launch into a NEW pane here" choice.** The panel decides; there is no
  chooser. If the two landings ever need to be picked by hand, the row belongs
  next to REACH and the panel's chrome height has to grow with it.

## Honest number, after the work

**6 was right.** The two gates were an afternoon; what took the time was
establishing which of four possible stories actually produced the session — the
composer mirror writing per keystroke is not something the code says out loud,
and the difference between "the bench targeted the wrong pane" and "the bench
targeted the right pane, which had no agent" changes the whole fix.
