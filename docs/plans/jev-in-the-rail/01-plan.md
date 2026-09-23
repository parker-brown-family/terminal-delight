# Jev in the rail — the combined gate

Product, architecture, program design and slices on one page, because a 6/10
buys one approval and not four.

---

## 1 · What it is for

The rail today reads git across every checkout a project's panes are in and puts
the result in the top row: a badge, and a ticker of frames. It is good. What it
cannot do is **judge**, and two functions carry the whole weight of pretending it
can:

- `ProjectState::frames()` — cyclomatic complexity 41. A hand-written priority
  ladder deciding what a person sees first.
- `ProjectState::is_calm()` — ten conditions ANDed. Any one of them trips the
  diamond, so the bar has exactly two opinions: fine, or not fine.

Both are rules standing in for a judgement. A rule that over-fires is a warning
nobody reads, and a fixed rotation shows the fifth-most-interesting fact as
often as the first.

**What Jev adds is not information. It is ordering, severity, and an honest
middle.** The rail keeps gathering; the model only says which of the things
already true deserves the one line, and how sure it is.

### What it will never do

Jev returns a Choice, a Score or a Noul. **It generates no text.** Every word in
the bar is still written by `engstate.rs`. Nothing about the prose changes, and
there is no path by which a model's sentence reaches the glass.

---

## 2 · The one architectural decision

**The judgement is a permutation and an annotation over frames git already
produced. It never creates one, never edits one, never removes one from the
record.**

```
scan (git, ~150ms, off-thread)  ->  ProjectState  ->  frames()  ->  Vec<Frame>
                                                                       |
                                          judge (network, budgeted)    |
                                          Option<Judgement> -----------+
                                                                       v
                                                       ordered, annotated, or NOT
```

Three things fall out of that shape, and they are the reason it is worth
building this way:

**The fallback is not built — it is what happens.** `Judgement: None` means
`eng_frames()` returns exactly what `frames()` returned, in exactly its order.
Today's bar, byte for byte. There is no second code path to keep in step, no
degraded mode to test separately, and a plugin being disabled is indistinguishable
from a slow network is indistinguishable from a machine with no key. All of them
are `None`, and `None` is good.

**Nothing can be invented.** Every candidate the model sees is a string the rail
already decided was true. The worst possible answer is a worse ordering for one
dwell. That is what makes a 56/72 model admissible here — it is picking among
true things, not asserting one.

**Unknown stays unknown to the edge.** `Option<Judgement>` is never
`unwrap_or_default()`-ed into a confident zero. The diamond therefore has four
states, not two:

```
  ◆  green    git says calm, and the model agrees                 (asked, answered, low)
  ◆  amber    the model is genuinely split — 0.4 to 0.6           (asked, answered, unsure)
  ◆  red      something wants a person before anything else       (asked, answered, high)
  ◇  hollow   not asked, or the answer is older than the reading  (UNDECLARED — not "fine")
```

The hollow one is the whole point. Today a bar with a dead endpoint would look
identical to a calm project.

### No HTTP client in Terminal Delight — and no transport of our own

`app/Cargo.toml` has no network dependency today: alacritty_terminal, futures,
polling, gpui, libc, serde, toml. That is a property worth keeping in a terminal
other people install.

**Amended 2026-09-23, before slice 2 was written.** This page originally
specified a budgeted subprocess of our own, shaped like `run_git`, calling the
Python client in `~/Work/jev-integrations`. That is superseded: a second agent
was already building the transport as a Terminal Delight **plugin**, and a
second one here would have been exactly the duplicate this plan exists to
avoid. The boundary below is theirs, quoted from the contract they sent rather
than invented here.

```
app/src/judge.rs  ->  plugins::discover() -> name == "jev"  ->  jev-mcp  ->  System One
```

- Discovery is `plugins::discover(&home)` looking for `name == "jev"`. Absent
  means not installed, which is the git-only path. **We add no resolver** —
  `resolve_jev_mcp()`, `builtin_jev()` and `jev_status()` are theirs.
- There is deliberately **no dev-layout fallback**: a plugin that makes a paid
  third-party call must not switch itself on because somebody cloned the repo.
- `jev_status()` makes no call, so it is the cheap check before drawing.
  `judge(state, questions)` is the general batched verb this plan uses.
- **Nulls are never defaults, and three absences stay distinct**: not installed;
  installed but no client or key (`available:false` with a reason); and asked
  but abstained (that answer null, with its own reason, while the others still
  stand). All three arrive here as `Judgement: None` or a `None` field, and none
  of them may become a zero.

That contract is the same line this plan draws between git's frames and a Jev
ranking, said from the other side: judgement and measurement never share a
field.

### Off by default, and off is the absence of a plugin

The plugin not being installed IS the off switch, so there is no second one to
keep in step. Parker installs it; anybody who does not has today's rail.

### One batch, one window, one scan

Every question goes in a single `ask_many` over one state. Measured: the budget
holds to about nine concurrent callers, and this machine runs twenty panes.
Per-pane calls would blow it, and would also pay for the state twenty times to
get the same answers. Batching independent questions over one state is the
design, not an optimisation.

### The staleness rule

The judgement is keyed on a hash of the reading it judged. When a scan lands and
the newest judgement's key does not match it, the bar draws the **git order**
and a hollow diamond — not the old judgement. An answer about a state that has
moved is a prediction, and the bar does not predict.

---

## 3 · The questions

Composed with the `ask-jev` skill at slice time, not from memory. The shape,
for approval:

**Noul — the diamond.** *A person returning to this window right now must act
before doing anything else.* One number, 0–1. Note the known catch: von reports
no confidence on a noul at all, so the amber band comes from distance from 0.5,
which is still an open decision in the jev-integrations repo. If it is unresolved
at slice 2, hosted Jev is the backend for this question.

**~~Choice~~ Score — the chooser. Amended 2026-09-23, before slice 3 existed.**

The original design was a Choice over the frame texts, verbatim: *which one line
earns a person's glance?* **That is the wrong primitive, and the source says so
without a single model call having to be made.**

`ProjectState::frames()` is an accumulator. After the calm early return there is
no further `return` and no `clear`, so sections 1 through 7 all push into one
vector: a non-calm reading emits an uncommitted-work frame *and* a branches
frame *and* one Shared frame per shared checkout *and* one Foreign frame per
wanderer, all true at the same time. Section 7 is worse than merely coexisting —
it is commented *"the derived sentence, last, so it lands after the facts it
sums"*, so it is a frame that **contains** several of the others.

A Choice picks one of a defined set and its distribution compares *competing*
options. These do not compete, and one is a superset of its neighbours. Asked as
a Choice, the mass spreads across several simultaneously-true answers and comes
back flat — which would read as *nothing stands out* when what actually happened
is *everything here is true*. Those are different states and the rail draws them
differently, so collapsing them is the unknown-is-not-zero failure wearing a
probability.

**So: one Score per frame, comparable across frames.** *How much does this one
fact deserve the single line a person will read?* Levels are concrete
situations, not adjectives. Code sorts by the score, which it already does with
a ladder — the model only supplies the rank. Falls out of it:

- **Dwell** comes from the top frame's own level, not from a distribution's
  concentration. A bar whose best frame scored low is a quiet bar.
- **"Nothing stands out" and "everything matters" stop being the same reading.**
  Every frame scoring low is the first; several scoring high is the second.
- Independent Scores over one state batch into a single request, which is the
  design anyway.

Credit where it is due: the question came from the agent building the transport
— *if two frame texts can both be true of the same reading, that is not a Choice
at all* — after their own flat 0.49 on an option set that was written as
independent sentences rather than as a partition.

**Score — collision severity.** Given two branch names, the panes working them,
and the files both touch: *how likely are these two lines to actually fight?*
Levels are concrete situations — both only touch a lockfile; they touch
different regions of shared files; they edit the same function. This is the
first one that **suppresses**, so it is also the first one that needs a measured
false-positive rate before and after.

**Noul — fidelity.** *This change set does what its branch name says.*

**Noul — waiting.** *This pane is blocked on a person rather than idle.*

---

## 4 · Slices, and what each one has to prove

| # | Slice | Proves |
|---|---|---|
| 1 | The seam | Git-only output is byte-identical to today, under test. Nothing calls out. |
| 2 | The call | A real answer from a real backend inside budget, and a killed endpoint draws a hollow diamond rather than a calm one. |
| 3 | The chooser | Order and dwell move with the distribution; a flat one silences the bar. Photographed at three distributions. |
| 4 | Collision severity | A false-positive count before and after, on this machine's real branches. |
| 5 | Fidelity, waiting | Each fires on a case built to fire it, and stays quiet on one built not to. |

Slice 1 is a tracer in the strict sense: it is the whole path with the
interesting part removed, and it ships the fallback as a finished thing.

### A standing rule, not a one-time pass

**Anything code can know, code answers — and the set of things code can know
keeps growing.** Two questions died that way already before either was written:
a pane's mode is a fact, and a pane already known to be awaiting input is a
fact, so neither reaches a model and both come back measured with no
probability.

The ones after those will not be obvious, and the failure is quiet: a question
that keeps working while its answer quietly becomes derivable, still costing a
call and still putting a probability on something now known. So **every shipped
question gets re-read when its surface changes**, asking only *can code answer
this yet?* — not once at the end of this plan, when the answer is trivially no.

## 5 · What would make this wrong

- **The bar starts lying quietly.** A mis-ordered ticker looks exactly like a
  correct one. Mitigation is that the model only reorders true things, plus the
  reading-hash key so it can never describe a past state.
- **It ends up in the paint path.** The scan is already off-thread; the
  judgement must be too, and `eng_frames()` must never block on one. A test that
  holds the subprocess open and asserts the frame still draws.
- **The amber band is theatre.** If distance-from-0.5 is not a real signal on
  the chosen backend, amber is a decoration. It gets measured at slice 2 or it
  does not ship.
- **Nine callers.** If more than one Terminal Delight window turns this on, the
  budget is shared. Worth knowing before a second window does.
