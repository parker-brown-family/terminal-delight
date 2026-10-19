# Fresh install opens flat — status

**Difficulty: 5/10.** Every piece is small. The risk is in two defaults. Nobody
runs a fresh install, so a wrong fresh-install look can go unnoticed. And a new
CRT switch can turn the CRT off on every machine that already has saved state,
unless an absent switch is read as "on".

Buys one combined plan page and one approval.

| Gate | Doc | State |
|---|---|---|
| Combined plan | `01-plan.md` | approved 2026-09-25 |

Approved with four answers, each the recommended option: contrast built at +25
and photographed at +10/+25/+40 before it is settled; a colour-set tile replaces
the palette; reset clears the main gauges only; the power-on flash follows the
CRT switch. The document was not amended.

## Built — 2026-09-25

Pull request 821. 1873 tests pass; clippy and rustfmt clean; four deliberate
mutations (absent key reads off, no flatten, fresh warp from the old dial, reset
clears warp) each turn a test red.

Photographed from hidden fresh-install windows, which settled the contrast
question differently than planned: from +10 to +40 the brightest text only
climbs from 93 to 108 of 255 grey at brightness −24, while brightness −10 takes
it to 145 and 0 to 173. The review page recommended brightness −10 with
contrast +25 (`reports/2026-09-25-fresh-install-flat.html` in the primary
checkout).

## Landed — 2026-09-25

Parker asked for a demo in a fresh instance, looked at the flat window at
brightness −24 and contrast +25, and said *"Looks amazing - SHIP IT!"*. The
defaults shipped as built, and the −10 recommendation was not taken. Merged as
`08a0a4a` after main was merged in and the whole suite run on the result (2039
pass). Installed as `td-08a0a4a-fresh-flat`, with one test skipped: the
skill-fixture test, which failed only because another session had uncommitted
edits in the brief skill (issue 850).

Follow-ups from the same thread:

- The docs homepage reel was re-filmed on this default (pull request 854,
  deployed).
- The public demos still show the tube: issue 849.
- Filming found a pane-freeze bug under heavy picture output: issue 853.
- **The coloured haze under every pane header** on the flat screen was the
  header's bloom, the one glow this change kept, cast onto the glass. In
  hindsight it belonged with the tube, so it now follows the CRT switch
  (`pane::header_shadows`).

**Post-hoc score: 5, held.** The gate drew no amendments. The one open number
was settled by eye on the build, as the plan scheduled. The miss was in the
look: keeping all of glow let one piece of the tube's bloom through, and only a
photograph of a flat window at full size showed it.
