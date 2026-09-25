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
it to 145 and 0 to 173. Recommended in the review page: brightness −10,
contrast +25. **Merge waits on that pick.** Review page:
`reports/2026-09-25-fresh-install-flat.html` in the primary checkout.

Post-hoc score: 5 held so far. The gate drew no amendments, and the one number
that is moving is moving on a measurement taken after the build, which the
plan had scheduled.
