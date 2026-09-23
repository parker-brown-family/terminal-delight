# Status: Jev in the rail

**Difficulty: 6/10** — a network-dependent semantic layer on the most prominent
row of the window. What keeps it off the 7+ shelf is that the thing it replaces
is the thing it falls back to: git. Unwinding costs a config flag, not a
rewrite. → **one combined plan page, one approval.**
**Turned out to be:** _(the honest number, filled in at the end)_

Opened 2026-09-23 on Parker's instruction: *"1. Make the fallback state current
git — 2. Let's start in on making it this SUPER COOL FANCY Jev'd out bar."*

- Combined gate — Product + Architecture + Design + Slices: `01-plan.md`,
  written 2026-09-23. **Awaiting the one approval.**

## Slices

- [x] 1 · **The seam.** `judge.rs`, a `Judgement` that is `None` by default,
      threaded through `Reading` so `eng_frames()` and the badge read it.
      Nothing calls out. 18 tests; two mutations — dropping the stale-reading
      guard, and letting the model talk git into green — each caught by exactly
      the one test that claims it.
      **Gates: clean.** `cargo clippy --locked -- -D warnings` exit 0,
      `cargo fmt -- --check` exit 0, 18/18 judge tests, and 1626/1628 of the
      whole suite. Run on the union tree — these files over `720a8d7`, the
      transport commit — because that is the tree they will merge onto, and the
      earlier standalone harness could not answer "does the crate build".
      The two red ones are `paneident`'s, already filed twice as #592 and #605:
      they walk the real process tree and find a live `claude` under the same
      parent instead of their own stand-in. Confirmed here rather than assumed —
      the pid they found is this session's own agent, and the pid they expected
      moves on every run.
- [ ] 2 · **The call.** Through the `jev` plugin's `judge()` verb, not a
      transport of our own — see the amendment in `01-plan.md`. One Noul. The
      diamond gets its four states.
- [ ] 3 · **The chooser.** **Amended before it was built:** one comparable
      Score per frame, not a Choice over frame texts. `frames()` accumulates —
      a non-calm reading emits several simultaneously-true frames and section 7
      is a sentence that *contains* its neighbours, so a Choice would spread
      its mass and read flat. Order and dwell come from the top frame's level.
- [ ] 4 · **Collision severity.** The first suppression. Measured false-positive
      count before and after, or it does not land.
- [ ] 5 · **Fidelity and waiting.** Branch name vs its own diff; a pane blocked
      on a person vs a window nobody is in.
- [ ] 6 · **The typed bar.** Intent routing with confidence-gated copy chips.
      Separate plan if it survives slice 3.

## The numbers this rests on

| Fact | Value | Source |
|---|---|---|
| Hosted Jev, per 1k input tokens | $0.000042 | TypeSafe `models.md`, $42/bn |
| A call every 10s, one window | **1.5¢/hour**, 36¢ for a full day | arithmetic on the above |
| Hosted p50 over the internet | 177 ms | jev-integrations, 2,600 calls, 2026-09-22 |
| Local von over Tailscale, warm | ~110 ms | jev-integrations slice 1 |
| Concurrency budget | holds to ~9 callers | jev-integrations slice 4 |
| Open-Jev 2B on JevBench | 56/72 | jev-integrations slice 6 |

The concurrency number is why this is **one batch per window per scan**, never
per pane. The JevBench number is why nothing here takes an action.
