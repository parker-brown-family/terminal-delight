# Status: The workbench follows the agent, not the pane

**Difficulty: 7/10** — it re-keys what a stored surface *means*, from "the pane
it landed in" to "the conversation that made it". A wrong answer shows one
agent's work on another agent's bench; that defect ran across fourteen benches
on 2026-09-18 and a person caught it by eye, not a test. Four gates.

**Turned out to be: 7/10, and the score was right for the wrong reason.** I
justified it on the identity re-keying being expensive to unwind. That half was
mostly settled by an existing resolver. What actually earned the seven was the
number of ways a *silently wrong* answer could be produced and look correct —
four amendments landed on the gates after approval, and a self-review before
merge found nine more defects, seven of them in code already tested and
mutation-tested. Every one was a value that was plausible, cheap and wrong.
See `05-retro.md`.

- Gate 1 — Product: **APPROVED 2026-09-21**, with two amendments taken at approval
  (agent-unique store key with no cross-agent access; a persistent per-turn
  record of the human's own prompts). Both are in `01-product.md`.
- Gate 2 — Architecture: **APPROVED 2026-09-21** by Parker's scope decision —
  *build the store half now, hold the four screens* — which is a direction to
  build from this doc. Amended twice before approval: the first-sweep sentinel,
  and the no-migration decision.
- Gate 3 — Program Design: **written 2026-09-21**, store half only — `03-program-design.md`, awaiting approval
- Gate 4 — Slice plan: pending

### Scope, as decided

| Built now | Held until the decoupled API has a shape |
|---|---|
| `conversations/<root>-<seq>/` store, `turns.jsonl` | screen E — below-the-line arrivals |
| the sentinel, and the file-then-remove invariant | screen F — the fresh bench |
| `Bench` gains a conversation key; clear on departure | screen G — the stale-version message |
| the ledger reader (`tenancy_for` / `tenancy_of`) | the "11 surfaces · bring it back" line |

Nothing held is discarded: those screens are designed, approved at Gate 1 and
waiting on `docs/plans/workbench-drives-the-agent/`.

### Reworked against the decoupling premise, 2026-09-21

> The workbench is no longer a direct mirror of the terminal. It uses an API to
> convey human interaction to the terminal process. — Parker

Designed inside that now, not braced against it. What moved:

- **The ask is authored, not scraped.** The composer holds the draft and sends
  it, so `turns.jsonl` records what this window did rather than what it saw.
  Gate 3's largest open doubt is retired; the scrape stays as the terminal
  face's fallback. `benchstore::ask` takes either source, which is what lets the
  store half ship now and improve when the boundary lands, with no migration.
- **The store is unaffected.** `benchstore`, `ConvKey`, the sentinel and
  file-then-drain touch no pane, no window and no gpui. Same code under either
  architecture — which is why this was the half to build first.
- **The raw-byte path has a proposed shape**: `interrupt()` and
  `answer_prompt(choice)` rather than one generic keys verb, each naming a
  situation and each knowing what retires it. Sent to the decoupling pane; it is
  their Gate 2 to decide.

**Sequencing, stated plainly.** The store half is complete-and-mergeable on its
own and does not wait for anything. The four screens are drawn by a bench that
does not yet have a place to draw them — that plan is at Gate 1, unapproved — so
they land with it rather than ahead of it. That is a dependency, not a descope:
they are designed, approved here, and specified.
- Gate 3 — Program Design: pending
- Gate 4 — Slice plan: pending

## Slices

_(Gate 4)_

## A THIRD plan is upstream of this one — read it first

`docs/plans/workbench-drives-the-agent/` (created 2026-09-21 08:06, difficulty
9/10, Gate 1 awaiting approval) separates the workbench from the terminal view
and puts an API between them. Parker's decision, annotated on
`~/Downloads/2026-09-21-workbench-text-entry.html`, with *"no more fixes to the
mirror"* and *"don't step over our bounds because a re-work will be ultimately
necessary."*

That plan is upstream of this one and this one is its prerequisite: a bench not
drawn in a pane has no pane to key by. See the decoupling section in
`02-architecture.md` for what each owes the other, and for the one open question
between them — whether the new API keeps a path to the pseudoterminal, which
`END SESSION` needs and the four screens do not.

## A second pane is working the same feature

The pane on tab 3 ("Workbench history association with agent ID") wrote
`reports/2026-09-21-bench-tenancy.html` — an architecture and gap-analysis brief
on this same feature, started independently and finished after Gate 1 here. It
also built `scripts/td-agent-ledger` + `.test.mjs` (18 tests, verified), which
records the SessionStart `source` word and a root/seq/prev lineage chain. That is
the instrument that answers the `/clear` question mechanically.

**This folder is owned here.** `00-status.md`, `01-product.md`,
`02-architecture.md` and `mockups/` were written in this pane; the other pane
offered to write `02-architecture.md` and was asked not to. Its material is folded
in with attribution instead.

Adopted from it: the `<root>-<seq>` key for `/clear`, and the shell-pane
carve-out. Migration is settled — no migration, no fold, one stale-version
screen.

**Parker's seven annotations are real and citable, and they are NOT in the
repository copy of the brief.** The decision-brief save button downloads an
annotated copy; the served file never changes. The record is:

```
~/Downloads/2026-09-21-bench-tenancy.html
```

106,478 bytes, 07:46:43 — against the repo copy's 94,821 bytes at 07:20:36. The
notes live in a JSON island at `id="report-notes"`, seven anchors, stamped 14:38
to 14:46. Grepping the repo copy finds nothing and looks exactly like "he never
annotated it". Read the download.

**One task, one pull request.** Two panes have now produced planning on this
feature. Whatever gets built collapses to a single pull request before review.

## Notes for a fresh session

**Read these before touching anything.**

- **This worktree is 65 commits behind `origin/main`** (`~/Work/terminal-delight`,
  on `fix/the-second-click-backs-out-to-your-branch`, 2 commits ahead and
  unmerged) — it was 58 behind when this folder was started, three hours earlier.
  Everything below was read from `origin/main` via `git show`, not from the files
  on disk here. The build happens on a tree at main.
- **The identity half already exists and is merged.** `app/src/paneident.rs`
  (PRs 576 and 581, 2026-09-19) binds every pane in one pass to the conversation
  it is in, labels each binding `Declared` / `Birth` / `Sole` / `Guess`, and
  exposes `certain()` — the subset a reader may attribute work to. Its module
  doc already states the rule this feature needs: *"a pane this module cannot
  bind gets no entry at all, and a caller that finds no entry must show nothing
  rather than fall back to a directory-wide guess."* This feature is wiring an
  existing resolver to the bench's store key, not inventing identity.
- **The previous plan deliberately decided the opposite**, and this supersedes
  it. `docs/plans/bench-kill-and-relaunch/00-status.md` shipped with *"The
  surfaces the ended agent made are still readable; the bench does not clear
  them to earn the offer"* in its Done-when, and left *"whether the ended
  agent's surfaces should be visibly marked as belonging to a finished
  session"* in its Open list. Parker has now decided it the other way.
- The `Bench` doc comment (`workbench.rs:2306`) says *"Per pane, deliberately"*.
  That sentence becomes wrong the moment Gate 4 lands and must change with it.
