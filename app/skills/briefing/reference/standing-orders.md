# Standing Orders

An agent loses its bearings in three places: when a slice starts and the plan is three files back, when its context is compacted and the chat that held the agreement is gone, and when it is about to say done. At each one, re-read what was approved instead of remembering it.

```bash
python3 scripts/brief.py orient docs/plans/floor/closing.html
```

```text
ORIENT  floor-closing · summary · revision c5a7f396ae92
  derives from  floor-plan (plan.html) · revision f83e9743c6a5

STANDING  approved; keep to these
  [call-footer-at-28-px]  Footer at 28 px
  [q-reserve]  Opening a draft never moves the page   · from floor-plan

NOT STAMPED  recommended, never approved; acting on one is a judgement call
  [q-fold]  Fold the builder into a slim bar   · from floor-plan

OVERTURNED  the person said no; do not build on these
  [call-accent]  Theme accent for the album page   · "use the label's own teal"

NOTES  the reader's words on these briefs; answer each before relying on the passage
  [q-fold]  "not without asking me"   · on floor-plan

OPEN CALLS  carrying on as if the person concurs
  (none)

ASKED  stopped, waiting on the person
  (none)

SETTLED  yes
```

## What It Reads

- **The brief's decisions.** Every `.ask`, `.grill-q`, or `.q` inside a `.grill`, by anchor and title: the elements that take a CONCUR stamp. A stamp made on this version of the page, or a `concurred` verdict in the calls ledger, makes one standing. A stamp made on another version is listed apart, as stamped on another version, because the words it agreed to may be gone; a stamp made after the page was last built counts, to the minute, and one made in the build's own minute stays unknown. A rebuild that changes nothing keeps the earlier build time, and a page edited by hand since its build cannot place any stamp on a version. Times are compared in UTC; a stamp with no zone is UTC, as the notes engine writes it, and a stamp that cannot be read never counts. For a call, the ledger decides: a later overturn there outranks a stamp left on the page. Anything else is a recommendation, and acting on it is a judgement call. If that call would contradict something standing, it stops: pass the standing anchor with `--conflicts`.
- **The notes on it.** Format 1 does not record which notes have been answered, so every note is listed.
- **Its parents.** Each brief named in `derives_from`, followed upward, with what each approved. A parent that has gone is printed as unavailable rather than skipped, and a derivation that loops is read once. Name parents when the brief starts: `new --derives-from PLAN`.
- **Its calls.** Open, asked, overturned, and answered calls whose evidence has changed since, from this brief's ledger and from any ledger a parent names, each marked with the brief it came from. An answered call stands whether or not the page still shows its row. With no ledger named, it says so; that is not the same as having no calls.
- **Rows that do not show their calls.** A call row whose words or pictures differ from its ledger entry, or that names a call the ledger does not hold, is listed as mismatched; neither its stamp nor the ledger's verdict counts for it.
- **Whether the work has settled.** `yes`, `no`, or `unknown` with its reasons. A missing parent, a ledger a brief names but that is not there, a row that does not show its call, or this brief naming no ledger makes it unknown, never yes. Open calls in this brief's ledger or in a ledger a parent names make it no. A parent that names no ledger adds nothing, unless it shows call rows: a plan brief usually has no calls of its own, and treating that as unknown would leave every brief derived from it unsettled forever, but a parent that shows calls and names nowhere to answer them makes settlement unknown.

`--json` gives the same tree for a host that follows the work. A host that wants a gate uses the exit codes: `calls` exits 0 when the work has settled and 2 when it has not, and `call` exits 3 when the policy says stop.

## Markdown

A Markdown brief marks a decision with an explicit anchor named `q-…` or `call-…` on the line above its heading, in single or double quotes, or with a block id at the end of the heading line, `## Fold the bar ^q-fold`. Agreement comes from the `agree` stances in the adjacent `name.feedback.json`.

## Where It Comes From

T3 Code's run carries a pointer to the plan that authorised it, and refuses to start a run whose plan does not exist. `derives_from` is that pointer, kept in the brief itself, so it survives the session that wrote it.
