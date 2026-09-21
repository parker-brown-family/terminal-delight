# Status: the response card's registers as grouped tabs

**Difficulty: 5/10** — one renderer and one state swap, contained to three files that no
other surface shares, plus a single genuine design decision (what the groups are, and
whether TDSP gains a field) that would be expensive to get wrong because it becomes the
navigation every reply is read through. That buys one combined plan page and one
approval, not four gates.

The plan page is the decision brief:

```
/home/parker/Work/reports/2026-09-18-response-registers-as-tabs.html
```

It carries eight figures, the three candidate shapes with a recommendation, the
exhaustive case table, and the four questions in its grill. Parker annotates it in the
browser and exports a note map; those notes are the approval channel.

## The brief, 2026-09-18

Two screenshots of a live response card and one sentence: *"I want to restyle the items
in responses… the agent to UI JSON structure MAY OR MAY NOT have to change… not sure —
but the idea is that these occupy too much space… we want similar to the right bar, folder
tabs that go across the top dividing them into groups. We will think about what these
groups are."*

This is the last unchecked slice of `response-overview` — *looked at on a screen* —
arriving with a finding. That slice was deferred because both outputs were DPMS-off when
the demo window was staged; it stayed open, and this is what it came back with.

## What was found, before anything was proposed

- **Every register is the same framed panel whether or not it is open.** `sk.panel()` with
  `py(8.)`, an 8-point gap to the next, the label at `Step::Body` (12 points before the
  pane's text gauge). Six registers is six frames and five gaps before a word of body.
- **Measured off the capture:** the card runs about 850 of 1041 rows. Two of its five
  registers are folded shut and each still costs about 48 rows for two words.
- **`emphasis::shelf()` has exactly one caller** — `benchdraw.rs:888`, the response card.
  The "at most one row is lit" machinery is not shared with any other surface, so it is
  free to change.
- **The tab look already exists.** `benchdraw::shelf_tab()` is four lines around
  `sk.chip(active)`, and it is what the rail's `overview / artifacts / decisions` strip is
  made of. The restyle borrows it rather than building a second one.
- **`Register` already carries the taxonomy** — eight variants, with `Register::known()`
  folding about thirty spellings onto them. A group is a pure function of a register, so
  nothing new has to cross the wire.

## Approved 2026-09-18 — all four, as recommended

Parker, on the brief: *"I go with 100% recommendations - lgtm. SHIP!"* Every
recommendation below is now the decision, not a proposal. Nothing in the grill
is outstanding; the next open question is whether the chip row survives a narrow
pane, and that is answered by looking at it rather than by asking.

## The decisions (were: proposed, amber — now settled)

- **Groups:** `reading` (tldr, eli5, layman, technical) / `evidence` (evidence, doubts) /
  `next` (asks, next), plus `other` only when an agent sent keys this build does not know.
- **Two levels:** the tab strip is the group; a quieter chip row picks the register inside
  it, and one body is shown. Reading registers are alternatives, and the accordion's whole
  cost came from treating them as a checklist.
- **The doubts** keep their count on the `evidence` tab, in the doubt colour. They are
  never folded today on purpose, and a tab is a click — this is the one place the restyle
  can quietly undo a decision.
- **No wire change.** An optional per-section `_group` escape hatch is named and
  deliberately not built.

## Load-bearing, and not to be undone

Three decisions from the response-feed pass survive this change unchanged: the gist is
always visible, the summons is drawn above the strip and never behind a click, and an
unknown key is kept and labelled rather than dropped. No number comes back beside a label.

## Slices — none started

- [ ] `Group` enum and `Group::of(Register)` in `surface.rs`, with a table test over all
      eight registers.
- [ ] The selection state in `workbench.rs`: which tab per surface, which register per tab.
      Both absent by default, because absent means the reader has not chosen and that is a
      different fact from choosing the first one. Forgotten on retire.
- [ ] `benchdraw::response()` draws a strip, a chip row and one body;
      `Hit::ToggleSection` becomes `Hit::PickTab` and `Hit::PickRegister`, routed in
      `pane/bench.rs`.
- [ ] The suppression rules as tests that fail on the parent: no strip for one group, no
      chip row for one register, and the `other` tab drawn only when it holds something.
- [ ] Looked at on a screen. The last one of these was deferred and stayed deferred for a
      day; the invalidation criterion is in the brief's method note.

## Difficulty, after the fact

Not yet written. Fill it in when the slices land.
