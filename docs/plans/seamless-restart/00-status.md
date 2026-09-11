# Seamless restart — status

**Difficulty 8/10.** A cross-cutting ownership and lifecycle invariant spanning
pane creation, attach, restore and deploy, where a wrong fix is invisible until
a relaunch costs real conversations. Normally four gates. Written as artifacts
rather than approvals because it was started while Parker was away; nothing here
is implemented beyond what the "Landed" section names.

Opened 2026-09-11.

## The goal, in one sentence

**Replacing either half of Terminal Delight — the window or the host — costs
nothing but the process being replaced.**

Today the window half is nearly there: it was bounced five times on a live
20-pane session on 2026-09-11 with zero terminals lost. The host half is not
close: replacing it ends every terminal in the session, which is why an encoder
fix cannot ship (#387).

## The invariants

Each is a property, each is mechanically checkable, and each has a ticket where
it does not hold.

| # | Invariant | Holds? |
|---|---|---|
| I1 | In hosted mode the client process owns no pseudoterminal | no — #377 / #382 |
| I2 | Every layout leaf maps to exactly one host pane, and every host pane to at most one leaf | no — #379 |
| I3 | Attach never invents a fact about a pane it has not measured | **yes** — #383, landed as #384 |
| I4 | The host outlives the window's cgroup, scope and session | no — #372 |
| I5 | A relaunch adopts a surviving host rather than starting beside it | no — #369 |
| I6 | The host can be replaced without ending a terminal | **no, and nothing tracks it** — see 01-plan |
| I7 | Host and client cannot silently pair on incompatible `gridwire.rs` | no — #387 |

I6 is the one nobody had named, and it is load-bearing for the others: until it
holds, every host-side fix is undeployable and every deploy is a decision about
whether the fix is worth the session.

## Ordered work

Ordering is by *what unblocks what*, not by size.

1. **#382 slice 0 — the ownership chokepoint** (I1). `make_pane_in_mode` plus
   three call sites, spec already written and approved in #382. Unblocks: every
   relaunch stops costing panes, which is what makes all later verification
   cheap. Do #382 slice 1 (three-valued hosting state, #380) in the same PR or
   immediately after — slice 0 widens the silent fallback from one site to all.
2. **#379 — one leaf, one pane** (I2). Distinct defect, same attach path.
   Currently repaired by hand with a script; the host de-duplicates spawn by
   recipe, so two recipe-less leaves carrying the same `resume` are handed one
   pane and the loser paints nothing.
3. **#381 — the survival harness leg** that stages a pane created *after* the
   window opened. This is the automated backstop for 1 and 2, and its absence is
   how #377 shipped. Must be shown to fail against a reverted slice 0.
4. **I6 — host handover** (see `01-plan.md`). The architecture piece. Unblocks
   #386, #378, and anything else host-side, permanently.
5. **#387 — refuse a skewed pair** in the meantime. Cheap, and stops the class
   of afternoon that produced it.
6. **#356 — the divergence itself**, which is still uncaused. #386 fixed four
   real encoder defects and demonstrably none of them is this. Needs a
   reproduction before it needs a fix.
7. **#369 / #372** — host lifecycle. Lower only because 1–4 change how they are
   approached, not because they are less real.

Deferred and explicitly not in this plan: #385 (hidden-resize drift), #380
standalone, #320 (the redeploy skill describes a layout that no longer exists —
fold into 4).

## Landed 2026-09-11

- **#384** — attach declares the host's geometry instead of a hardcoded 100×28
  placeholder, which was SIGWINCHing live agents to a size nobody measured.
  Merged. Satisfies I3. It did **not** affect the divergence, and the claim that
  it would was retracted in #383.
- **#386** — four defects in `encode_snapshot`, a generated sweep that found
  three of them, and a harness that checks a snapshot against the host that
  produced it. **Open, and must not be installed until I6 or #387.**
- **#387** — the skew hazard, filed after causing it: a half-applied `grid_hash`
  change took the diverging pane count from 11 to 13.

Five client bounces on the live session, zero terminals lost. The window half of
the split works.

## Honest post-hoc score

To be filled in when the plan closes. Early read: the 8 was right about the
lifecycle work and wrong about the diagnosis, which ate the afternoon and
produced two retractions — both caught by measurement rather than review, which
is the part that worked.
