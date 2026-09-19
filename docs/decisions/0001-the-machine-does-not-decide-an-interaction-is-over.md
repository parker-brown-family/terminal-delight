# 0001 — The machine does not decide that an interaction is over

Date: 2026-09-19
Status: accepted
Evidence: `reports/2026-09-19-four-ways-off-the-bench.html` (the drawn assessment,
with the twelve instances and the timeline)
Arises from: pull request 559, the fourth automatic exit off the workbench in
the surface's first four days

A place a person has put themselves is theirs until they move it. The workbench
face they are looking at, the skin they selected, the row they highlighted, the
composer they opened, the card they are scrolled into, the box they are typing
in. **No machine event may reset any of it**, and "the person has dealt with
this, take them back" is not a thing this codebase is allowed to conclude.

This is the first decision record in this repository. It exists because the same
mistake has now been made twelve times by twelve different people-who-were-us,
and every one of them was caught by Parker using the thing rather than by a test.

## What happened four times in four days

The workbench shipped on 16 September. Then, one per day, every day it had
existed:

| Day | The exit | Removed in |
|---|---|---|
| 17 Sep | escape retired a question an agent was waiting on | `ae44411` |
| 17 Sep | pressing send in the composer flipped the pane to the terminal | `ae38d42` |
| 18 Sep | escape, with nothing left to dismiss, flipped the pane to the terminal | `8a73ce2` |
| 19 Sep | answering a declared surface flipped the pane to the terminal | pull request 559 |

Parker, on the fourth: *"when I am in workbench and I make a choice the focus
SNAPS back to TERM … if I am in workbench I should stay locked in unless I
specifically step out."*

They were four bugs and not one because **the exit was decided at each call
site**. `set_face` was a plain setter, reachable from anywhere in the pane, and
four pieces of code each privately concluded that the person was finished. None
of them could see the others; each was fixed on its own, with its own reasoning,
by somebody who did not know the other three existed.

The fourth was also the surface disagreeing with itself. A question observed in
the terminal carries a cursor and dispatches through `Dispatch::Keys`, which has
never moved the face. A question or decision the agent *declared* dispatches
through `Dispatch::Tell`, which did. The two cards draw identically on the
bench, so the thing deciding whether a click threw you out of the room had no
representation anywhere on screen.

## The twelve

Beyond the bench's four, the same sentence — *a machine event reset something
the person had put there* — appears eight more times, each already repaired in
its own commit with its own reasoning, none of them aware of the others.

| Shape | Symbol | What the machine did |
|---|---|---|
| **The exit is decided at the call site** | `bench_act` | answering a surface turned the pane to the terminal |
| | `bench_send` | pressing send turned the pane to the terminal |
| | `Peel::Face` | escape, with nothing left, turned the pane to the terminal |
| | `peel` | escape retired a question an agent was waiting on |
| **A machine event revoked a selection** | `skin.rs` poller | saving `skin.toml` revoked an explicitly chosen skin |
| | `LogoPicker::merge` | a later search tier yanked the highlighted row |
| | `Bench::apply` | retiring a surface opened its neighbour |
| | `surfacefeed` sweep | an arriving surface selected itself and moved the shelf |
| | `plan_focus` | the focus watchdog took the keyboard out of an open box mid-word |
| **A machine event moved what the eye had learned** | `embodiment` | the bench shrank to a summary when the pane lost focus |
| | mother-bar badge strip | an agent finishing reshuffled the badges |
| | pinned-note badge | the pin changed slot as the roster changed |

Twelve is a floor. They were found by grepping for the comments that record each
repair, so a fix that left no comment behind is invisible to that method.

The grouping into three shapes is a reading, not a property of the code. The
instances are real and each was read at its site.

## The decision

**Person-owned state changes only on a person's gesture, or on a script saying
so in as many words.** Everything else keeps what is there.

The governing sentence was already written in `workbench.rs`, in the comment
above `LiveMove`, and it is promoted here to a rule for the whole repository:

> if we cannot say what changed, change nothing.

Three consequences that decide the cases this keeps producing:

- **Absence of a signal is not a signal.** A parse that failed, a poller that
  found a file, a sweep that saw a new id — none of those is evidence that a
  person finished with anything. The evidence that a question is over is the
  agent no longer waiting; the evidence that a person left a surface is the
  person leaving it.
- **A collapse belongs at the front end.** A renderer may decide that a new
  arrival draws a marker, dims a row or shows a count. A store, a sweep or a
  handler may not decide it replaces what is on screen. This is the same rule as
  *unknown is not zero*, applied to attention instead of to data.
- **Being correct about the destination does not authorise the move.** The
  deleted flip was right that the reply would appear in the terminal. It was
  still wrong, because where to look next is not a thing the program knows
  better than the person looking.

## How it is enforced, and why not as a list

A rule written as prose at four call sites is four places to forget it, which is
the mechanism that produced the four exits. Escape stopped producing this bug
when its ladder became **one pure function** (`peel`) that every handler asks;
the face never got that treatment, and kept producing it.

So the rule is stated as a **place**, not as a set of cases:

```
nothing_in_the_bench_half_flips_the_pane_off_the_bench   (app/src/pane.rs)
```

It scans `app/src/pane/bench.rs` — everything the bench does with a click, a key
or a wheel — with comments stripped, and fails on any `set_face` or
`toggle_face` at all, naming the function it found one in. The gestures that
legitimately move the face live in `pane.rs` (alt+k, the TERM chip); a script's
`bench off` comes through `ctl`.

Two properties that are the point rather than the implementation:

- **It catches the function nobody has written yet.** A list guards the cases on
  it. A boundary guards the fifth exit, in whatever handler somebody invents for
  it next week.
- **It was mutation-tested.** The deleted line was replanted and the guard
  failed, naming `bench_act`. A guard never seen to fail would report success
  against a broken build. Comments are stripped first because the paragraph
  explaining the removal names both `set_face` and `Face::Terminal`, and a scan
  that cannot tell code from a description of code is satisfied by its own
  gravestone — which has happened in this repository before.

**When the next instance of this is removed, remove it as a place too.** The
repair is not "delete the line"; it is "name the region this may not happen in,
and gate the region."

## What this does not forbid

- **A person's own gesture, obviously** — alt+k, a chip, a key, a click.
- **A script that says so.** `ctl`'s `bench off` is a person at one remove.
- **Drawing attention.** A badge, a count, a marker, a dimmed row, a live strip
  saying an answer was taken. Telling somebody something arrived is the correct
  response to something arriving; moving them to it is not.
- **Refusing to keep state that no longer exists.** Closing the card for a
  surface that was retired is not a reset — the thing is gone. Opening its
  neighbour would have been.

## Left open on purpose

On the overview the newest reply stands in as the card without anybody opening
it, and the card's scroll resets whenever that stand-in changes. A long answer
you are halfway down is therefore replaced, at the top, when the next one
arrives.

It is the same shape and it is deliberately **not** fixed. Parker, on being
asked: *"leave it for now - scream test."* If it never screams, the overview's
feed behaviour was right and this clause is the record of having checked. If it
does, the repair is that a new reply arrives below the reader and the strip says
it landed — a small design rather than a deletion, and it supersedes this
paragraph.

The test itself is tracked — issue 565, *Scream test: the overview resets your
card scroll when a new reply arrives mid-read* — with the deadline and the
close-as-invalid criterion written into it, so the scream test is a thing that
finishes rather than a sentence that sits here forever. Whoever picks it up
runs the invalidation first.

## Scope

This record is about this repository. Whether it generalises to the
machine-global agent doctrine is a separate call and has not been made.
