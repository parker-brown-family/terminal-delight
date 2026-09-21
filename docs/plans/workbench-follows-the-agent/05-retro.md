# Retro: what I would do differently on the first pass

Written 2026-09-21, after Gates 1–3 were approved and the tracer slice landed in
a pull request. Only the things that would have changed the work, ordered by
what they cost.

**Difficulty score: submitted 7/10, and it held.** The check I proposed for it
was wrong, though. I said two bare approvals in a row would mean I had scored
high, and when both Gate 1 questions came back as my own recommendations I
flagged that as the signal. It was not. What actually measures a gate is whether
**evidence changed the document**, and four amendments landed after approval:
the filing key's precedence inverted, the clear rule turned out to be recorded
backwards, the first-sweep mark had to become durable and exact, and a fold was
withdrawn. An answer matching your recommendation says nothing; a gate nobody
amends says everything.

---

## 1 · I verified against a shared build directory

The green run I reported may not have contained my test.

`CARGO_TARGET_DIR=<primary>/app/target` turns a cold gpui build from twenty-odd
minutes into thirty-four seconds, and that is why it is in the notes. With
another agent building into the same directory it can also serve a **stale test
binary** — a sibling measured sixteen of their own tests silently absent from a
green run.

The tell was in my own output and I read past it:

```
Blocking waiting for file lock on build directory
```

I then ran a two-way mutation test on top of that binary and reported both
mutations as caught. They were, but the run proving it was the one I had no
reason to trust.

**Next time:** a private target dir for anything whose result I will act on, and
one command before believing a count.

```bash
cargo test -- --list | grep <the test you just wrote>
```

## 2 · I paraphrased a rule instead of quoting it

This would have emptied a bench on every compaction.

A peer sent me the ledger's branch rules in plain English. I summarised them into
Gate 2 as *"`seq` increments on a clear and on nothing else"* — the exact
inverse — then built a directory layout on the inversion and argued for it when
challenged. An agent that compacts at noon would have gone blank in front of the
person still talking to it, with the load succeeding and simply the wrong
directory read.

A fact survives paraphrase. A **rule** is a branch table, and a branch table
inverts under paraphrase while still reading fluently. Gate 2 now carries the
rules as a quoted source block.

## 3 · I reasoned from an empty grep

I told Parker one of his own decisions could not be cited.

A peer attributed four decisions to him. I grepped the brief in `reports/`, got
zero hits, found no note payload, and reported that the decisions were
unverifiable. They were in `~/Downloads` — the annotated copy the save button
writes, which is the only copy that ever carries notes.

Zero hits was the expected result whether he had annotated the page or never
opened it, which makes it the worst possible thing to reason from. One of my four
search strings also missed because he had typed `taht`, so even in the right file
a grep for somebody else's paraphrase would have under-reported.

## 4 · I turned my own permission refusal into a fact about the machine

This nearly removed a capability from every future session.

Two interpreter forms are blocked in this session's permission mode. Both
refusals end *"This is a permanent security restriction"* — no subject, no
mention of whose settings — so I repeated it into shared memory as a property of
the box. A peer in a permissive mode ran both, minutes later.

The general shape hit three times in one day between two agents, in three
directions, and the tell was identical every time: **the claim arrived without a
subject.** *"The only `remove_file` is…"* (true of a stale tree). *"Permanent
security restriction"* (true of one session). *"No more fixes to the mirror"*
(an agent's conclusion, quoted as Parker's).

## 5 · I asked for approval five turns running when nothing needed it

Parker had to ask *"so what are we doing -- waiting?"*

Gate approval blocks **code**, not the next document. Gate 3 needed nothing from
him and I had it written inside one turn once he pushed. Four of those five turns
ended with an ask that was really a status report wearing a question mark.

## 6 · I found the upstream plan three hours late, by accident

Gate 2 was written against an architecture that was already being replaced.

The workbench is being decoupled from the terminal view. That plan existed in the
repository, at `docs/plans/workbench-drives-the-agent/`, while I was writing a
Gate 2 that assumed the current coupling. I found it because I sorted plan folders
by modification time looking for something else.

One call, at the start, would have found it:

```bash
ls -dt docs/plans/*/
```

A plan folder states intent. A pane title states whatever was on screen when the
auto-topic was generated — the decoupling pane is titled *"Workbench text box
navigation and resize"*, which is a true description of a symptom by an agent
doing a nine-out-of-ten architecture change.

## 7 · The gesture has a hole I did not name in the pull request

Verified on this tree rather than recalled: `host.rs:1595` holds a pane at
`Claude` for as long as the **alternate screen is up**.

```rust
if was_agent && !still_agent && on_alt {
    return current.cloned();   // an agent shelling out is still an agent
}
```

That is right — an agent running `rg` must not rename itself twice a second. It
also means **a signalled agent never demotes**, because a killed process never
sends the leave-alt-screen sequence. `set_mode`'s departed edge never fires, so
the bench never clears.

So "an agent that ends takes its workbench with it" is true of every ending the
person can reach from the bench or by typing `/exit`, and false of `kill -9`. The
repair is already specified and deferred elsewhere: a host `Request::KillForeground`
plus a forced demotion on the window side, written up as rung two of
`docs/plans/bench-kill-and-relaunch` and held back because it needs the session
host upgraded and the host owns the pseudoterminals.

I knew this — it is quoted in my own Gate 2 — and still described the slice as
though the edge always fires. **A limit you know about and do not state reads
exactly like one you missed.**

## 8 · I staged files in a worktree that was not mine

Caught immediately and reset, with nothing lost. Worth recording because the
primary worktree is shared, sixty-five commits behind, and carrying another
session's unmerged branch — and `git add` in the wrong directory is one
keystroke from the right one.

---

## What went right, and is worth keeping

- **Every peer claim was checked before it was acted on**, and checking was cheap
  every time. Three of them were correct and one was correct-but-stale; the
  stale one would have shipped a wrong invariant.
- **The wrong versions stayed in the documents**, struck through with the
  reasoning. A later reader who sees only the final design re-derives the cheap
  version and reintroduces the defect — that is how the per-run first-sweep mark
  would have come back.
- **The tracer slice needed no store, no sentinel and no key**, so it was
  demonstrable before any of the contested parts existed.
- **The mutation test pulled in two directions.** Under-clearing and
  over-clearing both fail, which is what makes it a test of the rule rather than
  of the implementation.
