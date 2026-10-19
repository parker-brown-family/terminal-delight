# Layout

How a brief is arranged, and which components earn their place. Read this when deciding the
shape of a document, not when styling one.

## The disclosure ladder

Four rungs. A reader who stops at any rung has enough to act at that level of detail.

| Rung | What it is | Budget |
|---|---|---|
| 1 · **Headline** | The one sentence he needs if he reads nothing else. A verdict, not a summary. | ~40 words |
| 2 · **The scannable layer** | Verdicts, Read-first. What must be looked at, ranked. | ~4 minutes |
| 3 · **The page** | Sections that carry an argument each, and this is where the pictures live — a mockup, a diagram, a comparison, a table. | ~20 minutes |
| 4 · **Modals** | Everything a reader would ask "why?" about. Unbounded; nobody is forced through it. | as long as it takes |

The mistake is putting rung-4 material on rung 3. If a section runs past a screen or two on
the main page it is competing with the brief — move it to a modal and leave a one-line entry.

**Order matters.** Verdicts before Read-first: the reader wants to know the answer before he
is told what to scrutinise. Parker argued for this explicitly and he was right — a
"read this first" block that precedes the conclusion asks for attention it has not yet earned.

## The index that is not a table of contents

The cross-cutting findings list is the most useful component in a brief, and it works because
it maps to **conclusions**, not to layout. Each row is a claim with its evidence one click away.

A traditional table of contents tells you where things are. This tells you what was found. If
your findings list reads like section names, rewrite it as sentences that assert something.

## Components, and when each earns its place

None of these is required. Build what the material needs.

| Component | Earns its place when | Do not use it when |
|---|---|---|
| **A mockup figure** | The reader has to judge a surface — what it will look like, at the width he will read it | The surface already exists and he has seen it |
| **An SVG diagram** | The reader has to judge a mechanism — flow, geometry, ordering, resolution, sequence | The mechanism is two steps; a sentence is faster |
| **Verdict cards** | Several things are being compared and each gets its own call | There is one subject — a card of one is a header |
| **Read-first list** | The reader is time-boxed and items differ in urgency | Everything matters equally; then it is just the page |
| **Stat tiles** | One number per thing carries an argument on its own | The numbers only mean something next to each other — use a table |
| **A chart** | A comparison is genuinely visual and spans one order of magnitude | The spread is 4 orders of magnitude (243,648 vs 41 — that is a tile row, not a bar chart) |
| **Option cards, one lit** | Alternatives are being weighed and you have a recommendation — see *The recommended path* below | You have no recommendation; then lighting one is a lie |
| **Findings list** | There are cross-cutting conclusions that do not belong to any one subject | |
| **A grill section** | Real decisions remain that are the reader's to make | You are presenting a conclusion, not asking for one |
| **Modal sub-report** | A reader would ask "why?" and the answer is longer than a paragraph | |

The two figure rows are first because they carry the most. `reference/pictures.md` holds the
figure contract, the repertoire of eleven drawn from the attention-spine brief, what each
colour means, and the provenance modals that keep a drawing from reading as a measurement.

Before adding a chart, read the `dataviz` skill. One good chart beats four; a bad one is worse
than a table. If nothing is worth plotting, plot nothing. A chart and a diagram answer
different questions — a chart argues with quantities, a diagram argues with structure, and
most briefs need the second one more often than they reach for it.

## The recommended path, drawn as a path

Whenever a section lays out alternatives — A/B/C, "as specced" vs "re-cut", two depths of the
same feature — **mark the one you recommend on the card itself**, not only in the prose under
it. Parker's words, on the slice-4.5 brief that first did this by hand: *"visually seeing the
recommended path helps to weight decisions."* A reader scanning three equal-looking cards has to
read all three before he can start weighing; a reader scanning three cards where one is lit
starts from your recommendation and spends his attention on whether to overrule it. That is the
whole job of the section.

`base.css` styles `.cards`, `.card`, `.card.rec`, `.badge` and `.why` — the lit card glows
green with its badge — so the markup is all a brief needs:

```html
<div class="card rec">
  <span class="badge">recommended</span>
  <h3>B · contract core, flip, then undo</h3>
  <p>What it is.</p>
  <p class="why">Why this one — and what it costs you.</p>
</div>
```

Three rules, all learnable the hard way:

- **Exactly one card per group is lit.** Two recommendations is no recommendation, and the badge
  stops meaning anything the second time a reader sees it hedged.
- **The unlit cards still get their honest case.** A recommendation the reader cannot argue with
  is a decision you took from him. Each alternative says what it is genuinely better at.
- **Say the cost of the one you lit.** The lit card carries the "safest on paper; slowest to the
  thing the feature exists for" line too, or the badge is marketing.

`.card` is already in `notes.js` TARGETS, so a lit card is annotatable with no extra work — he
can disagree on the card itself, which is where the disagreement belongs.

## Scoring

If subjects are scored, say what each score answers, in one line, in the method section. A bare
7/10 is noise. From the intake audit:

- **Security** — how much of our machine does this get, and how carefully does it treat it?
- **Novelty** — does it do something we cannot already do?
- **Restraint** — is the engineering proportionate to the problem?
- **Maintenance** — will this still be here, and answered, in a year?

Scores are a summary of prose you have already written. Never the other way round.

## The grill

When decisions remain, ask them properly:

- Model the open decisions as a tree; the **frontier** is every decision whose prerequisites
  are already settled.
- Ask the whole frontier in one round, numbered, **each with your recommended answer**.
- A question whose answer depends on one still open in this round belongs to a **later round**.
  Say which those are and why they are waiting.
- Facts are your job, decisions are his — never ask for something you could look up.

Use `AskUserQuestion` for the round itself (cap: 4). Record the answers in the brief afterwards
so the document carries the decision, not just the question.

**Mark it up as `.ask` inside a `.grill`, and never invent a class name here.** The grill is the
section the whole document exists to get answered, so it has to be annotatable — and `notes.js`
can only tag what it has a selector for. Two briefs shipped with uncommentable grills because
each invented its own name (`grill-q`, then `q`) and neither was in `TARGETS`. Both are matched
now for the sake of those files; new work uses `.ask`:

```html
<div class="grill">
  <div class="ask">
    <h3>1 · The question, as a question</h3>
    <p>What is actually at stake, and why it is his call and not yours.</p>
    <p class="rec"><b>Recommendation:</b> your answer, with the reason.</p>
  </div>
</div>
```

**Every `.ask` takes a concur stamp**, which `notes.js` adds on its own: a space on the
decision's right, clear of the note button in the top-right corner. It forces the decision's
right padding, so a brief's own `.grill .ask` padding shorthand cannot run the question under
it. Write the decision so that a bare concur is a complete answer — the recommendation line
is what he is stamping, so it has to say what happens if he agrees.

When the round comes back answered, the grill does not get filled with invented questions to
keep it alive. Move the answers up into the settled record and say plainly that nothing is
waiting on him — then name the *next* decision and what triggers it, so it is parked rather
than lost.

## Verification checklist

Run before delivering. Screenshots crop, and return blank frames at deep scroll — the DOM does
not lie.

- [ ] `scrollWidth === clientWidth` (no horizontal overflow)
- [ ] every `[data-dlg]` resolves to a `<dialog>` that exists
- [ ] every dialog has non-trivial `.dlg-body` content
- [ ] `<script>` and `<dialog>` tags balance
- [ ] **every component you built is annotatable** — not "the count is non-zero". A count passes
      while the most important section is uncovered, which is exactly how two briefs shipped with
      uncommentable grills. Assert coverage per component and read the failures:

      ```js
      ['.grill .ask, .grill .q, .grill-q', 'figure', '.card', '.finding', '.tile',
       '.esc-list li', 'table.wide tbody tr', '.callout'].map(s => [s,
        document.querySelectorAll(s).length,
        [...document.querySelectorAll(s)].filter(e => e.classList.contains('notable')).length])
      ```
      Any row where present > 0 and tagged < present is a defect. A grill row of `[n, 0]` is the
      one that matters most. Note `tbody tr`, not `tr` — `notes.js` skips header rows deliberately
      (`el.tagName === 'TR' && el.querySelector('th')`), and an assertion that flags an intentional
      exclusion is an assertion people learn to ignore, which is how the grill defect survived.

### Two undocumented hooks in notes.js, since both are easy to get wrong

- **Read-first numbering is `.esc-n`, not `.num`.** With `<span class="esc-n">3</span>` present,
  the anchor becomes `readfirst-3`; without it you silently fall back to a four-word title slug.
  Both work, but the numbered form is shorter in the exported map and stable when you reword a
  heading.
- **Anything inside `#d-note`, `#d-export` or `.notebar` is skipped**, so the notes UI never
  tags itself. Do not reuse those ids.
- [ ] a note can be added on a grill item specifically, and the badge appears
- [ ] every grill item has a concur space — `[...document.querySelectorAll('.ask')].filter(a =>
      !a.querySelector(':scope > .concur-zone')).length === 0` — and a stamp shows up in the map
      as `✓ concur`
- [ ] the exported map has readable anchors, and reports its own token cost
- [ ] every group of option cards has **exactly one** `.card.rec`, and it names its own cost —
      `[...document.querySelectorAll('.grid, .cards, section')].map(g =>
       [g.querySelectorAll('.card').length, g.querySelectorAll('.card.rec').length])`
      — any row reading `[2+, 0]` or `[n, 2+]` is a section that asks the reader to weigh
      alternatives and refuses to say which way you lean
- [ ] a chart's longest label is inside the figure, not overflowing it
- [ ] grid columns do not orphan the last card (4 cards in a 3-wide grid looks broken)
- [ ] every `<figure>` has a numbered `.lbl` and a `<figcaption>` that asserts something —
      a caption describing what the reader can already see is a wasted line
- [ ] every `<svg>` has a `viewBox`, no fixed `width`, and an `aria-label`
- [ ] nothing external: `[...document.querySelectorAll('img, [src], link[href]')]` is empty
- [ ] the page reads at **968 px** (Parker's tiled pane) and at 390 px — wide diagrams sit in
      their own `.scroller`, and the page itself never scrolls sideways
