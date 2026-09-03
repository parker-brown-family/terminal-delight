# Handoff — agent-wall vitals bars (2026-09-02)

## Status

**Built, tested, installed. Not committed** — the tree is dirty by design; review
the diff before landing.

`~/.local/bin/terminal-delight` → `~/.local/lib/terminal-delight/td-8374836-vitals`.
**A running window holds the old inode, so TD must be restarted to see any of this.**

| Check | Result |
|---|---|
| `cargo test --bin terminal-delight` | 516 passed (472 pre-existing + 44 new) |
| `cargo clippy --all-targets` | clean |
| `cargo fmt --check` | clean |
| `scripts/td-vitals-oracle.sh` | **12 agree, 0 differ** |

## What landed

**Three bars on every agent card**, in the box that used to hold the message
feed. The feed was four clipped lines of whatever happened to be on screen; the
bars answer the question you actually open the wall for.

- **WINDOW** — how full the context is. Exact, not estimated: `input_tokens +
  cache_creation + cache_read` on the newest turn *is* the prompt that was sent.
- **FATIGUE** — accumulated damage, independent of size: compactions survived and
  what they dropped, a tool-error rate rising against the session's own baseline,
  the same file fetched a fourth time, latency per token of output drifting up,
  hours on the clock.
- **RELEVANCE** — what fraction of the *resident* window still serves the task.

**The pair that earns the room is WINDOW × RELEVANCE.** A full context means
opposite things depending on the second bar, and no token counter can separate
them: full of ballast is a cheap COMPACT, full of live detail is a HAND OFF, and
compacting the second destroys what is in use. That cross is the whole feature.

**The bar's visual rule.** Length is the number; colour is only *which direction
is bad*. A white→tone gradient is laid across the **track** and clipped by the
fill, so a 7% bar shows just the white origin and a full bar shows the whole
sweep. Sizing the gradient to the *fill* instead would paint a 7% bar fully
saturated. WINDOW carries a delayed ramp (`smoothstep(0.60, 0.98)`) so a
half-full context stays white — it is genuinely fine. No yellow, no blue, no
threshold flip: distance from white, in one of two directions.

**One provider chip: `OPUS · MAX`.** It replaces MODEL/EFFORT, which never once
showed a model — `parse_model` read a `--model` flag nobody passes, fell through
to the *program* name, and printed `MODEL CLAUDE` on all seventeen cards. The
transcript carries both, and is the only source that can say `max` at all:
`hud::extract_effort` scrapes the status line and knows just high/medium/low.
Version dropped, so `opus-5` and `opus-4-8` are both OPUS. Cross-provider via
`vitals::model_name` (OPUS/FABLE/SONNET/HAIKU/GPT/CODEX/GEMINI).

**A `CALL` chip and the ⚠ marker** appear only for STOP and HAND OFF. COMPACT is
a thing to do, not a decision, and a wall that flags every healthy agent teaches
you to read past the flags.

## Files

- `app/src/vitals.rs` — new. Metrics, verdict, fleet binding, CLI. 44 tests.
- `app/src/main.rs` — `agent_vitals` map, a 10s sweep, the bar render, the chip.
- `app/src/session.rs` — `claude_slug` / `proc_start_unix` widened to `pub(crate)`.
- `scripts/td-agent-vitals.mjs` + `.test.mjs` — the reference implementation (35 tests).
- `scripts/td-vitals-oracle.sh` — diffs the two.

## Two decisions worth keeping

**Whole-file parse, not a bounded tail.** The first design read a tail to keep
cost down. Measured, that was solving nothing: the biggest transcript (33MB)
parses in ~250ms and the whole seventeen-agent fleet in **560ms**, on a
background thread, only for files that grew. The tail would have truncated the
churn/retread baselines and dropped compaction scars older than the window —
buying imprecision for time nobody was spending.

**The JS stays, as an oracle.** These metrics have exactly one failure mode that
matters: a plausible *wrong* number. Nothing crashes, a bar draws, the call is
confident, and no unit test catches it because both sides are self-consistent.
Every divergence the diff found was of that kind, and none were visible from the
spec:

- seconds vs milliseconds — sub-second turn gaps fell out of the latency sample (FATIGUE ±3)
- the focus boundary read the *turn's* timestamp, not the call's; an assistant
  message with no `usage` block pushes no turn (RELEVANCE ±6)
- the call key joined every prose field instead of reading the first, so a Bash
  call's changing `description` split repeats of one command
- `app/src/` and `app/src` counted as two directories
- the jq expression `.conclusion//.state` was swallowed whole as a path
- `~/Work/x.rs` was a different file from its expansion (4.4k tokens misfiled)

All six are now Rust tests. Run the oracle after touching either side.

## Not done / next

- **`#272` — two same-cwd panes resolve to one session id.** Filed with proof.
  The wall's bars are safe: `vitals::assign` binds the fleet with mutual
  exclusion (declared `--resume` first, then birth match tightest-first, then
  last-spoken), so no two cards can draw one conversation. **`list_panes` and
  `pane_events` still collide** — the fix belongs in `session.rs`, whose
  `claude_transcript` falls through to `newest_jsonl`, i.e. newest by mtime.
  Note mtime is the wrong signal regardless: Claude Code writes bookkeeping
  records back into transcripts a conversation has already left.
- **Codex panes draw no bars.** `total_token_usage.input_tokens` and
  `turn_context.model` make WINDOW and the chip computable; the rollout schema is
  entirely different (`{ordinal, payload, timestamp, type}`), so it wants a
  second parser behind a shared shape. Deliberately not faked.
- **Not visually verified at a tiled 968px width.** The bars are fixed-width
  (label + 60px track + caption ≈ 134px at `cs` 1.0) inside an `overflow_hidden`
  box, and the label collapses to its initial below `cs` 0.95 — but nobody has
  looked at it on a narrow pane yet.
- The card ordering still sorts by `AgentState`, not by the call. A STOP agent
  gets the ⚠ and the chip but is not pulled to the front.

## How to verify

```bash
cargo test --bin terminal-delight --manifest-path /home/parker/Work/terminal-delight/app/Cargo.toml
```
```bash
bash /home/parker/Work/terminal-delight/scripts/td-vitals-oracle.sh
```
```bash
node /home/parker/Work/terminal-delight/scripts/td-agent-vitals.mjs --detail
```
```bash
terminal-delight agent-vitals --parts ~/.claude/projects/<slug>/<session>.jsonl
```

`--parts` dumps the six fatigue components and the four relevance buckets;
`--keys` dumps every call key. That pair is how each divergence above was
actually localised.
