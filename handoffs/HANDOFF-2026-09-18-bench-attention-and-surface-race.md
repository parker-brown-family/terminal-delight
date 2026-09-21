# Handoff — bench attention tiers and the surface-write race (2026-09-18)

## Status

**Landed and cut over.** `main` is `7e52557`; installed as
`~/.local/lib/terminal-delight/td-7e52557-main` with the symlink taken and
plugins reinstalled. Four PRs merged: **#534** (attention tiers + TDSP 0.4),
**#542** (test isolation), **#547** (surface-write race + adversarial suite),
**#548** (changelog dedup). Nothing of mine is uncommitted or unpushed.

## What's done

| Change | Verified by |
|---|---|
| `app/src/emphasis.rs` — one resolver for attention: `Emphasis` ramp, `facet()` the only place a tier becomes colour, `clothe()` fixing shape → tier → runtime | 11 unit tests; budget mutation caught by 3, inferred-flag mutation by 1 |
| TDSP **0.4** — declared `escalation` (`blocking`/`wanted`/`none`), `Option<Escalation>` keeping *absent* ≠ `none` | 5 parse tests incl. `an_undeclared_escalation_is_not_a_declared_none` |
| Escalation pinned above the title; registers flat and peer-equal; `Register::Tldr` | full suite |
| `emphasis::meta()` readable secondary ink; `Body::measure()` **deleted** | full suite |
| Light follows the last-opened register at half strength (`aglow_at`) | regression test for the old always-lit tl;dr |
| Empty bench is one dialogue card; `Anchor::Eye` | `an_offer_sits_at_eye_level_and_a_transcript_on_the_floor` |
| **Tests can no longer type into a live window** (`--pid 4294967295` pinned in the shared helper) | mutation: remove the pin → guard fails |
| **`drop_surface` race fixed** — temp name unique by construction (atomic counter + pid) | 16-thread regression test; adversarial suite green in parallel ×3 |

Final gate on the built tree: **1,296 + 9 + 4 + 8 + 9 pass, 0 failed**;
`cargo fmt --check` and `cargo clippy` clean.

## How to run / verify

```bash
cd /home/parker/Work/td-wrangle/app && cargo test
```
```bash
cargo test --test bench_adversarial          # must be run in PARALLEL — that is the failing mode
```
```bash
cd /home/parker/Work/td-wrangle && cargo build --release --manifest-path app/Cargo.toml
```
Cut over by pointing the symlink at the new `td-<sha>-main`; rollback is one
`ln -sfn` to `td-19baa8b-main`. Open a **fresh window** — live windows keep the
binary they already mapped.

## Not done / next

- **Register tabs are NOT mine — `terminal-delight-7c` owns them**, building on
  `bench/register-tabs` out of `~/Work/td-award`, from the approved brief
  `2026-09-18-response-registers-as-tabs.html`. My competing brief is retired
  (issue #552 closed). Do not start this.
- **#550** — `surface -` validates nothing; any JSON lands and the agent is
  never told. A contract decision, deliberately not taken in a stress pass.
- **#551** — an orphaned 210-line MCP theme-toggle removal, on no branch,
  rescued to `refs/attempts/rescued-2026-09-18/td-attention`. Needs its author.
- Checkable asks: designed (typed per-surface state record), not built.
- The keyboard cannot move the lit register — `shelf()` takes the focus argument
  and every caller passes `None`.
- Decision, changeset, table and the composer still pick their own inks.

## Watch out

- **Run any new suite in parallel at least once.** The write race failed two or
  three cases per run, a different set each time, and passed every time
  serially.
- **Never `git add -A` in a shared worktree.** `~/Work/td-attention` held another
  agent's uncommitted files the whole session; I committed file-by-file.
- **`cargo fmt` across the tree reformats other agents' in-flight files** — use
  `rustfmt <your files>`.
- **A test driving a UI verb must pin an impossible `--pid`** and assert the
  failure names it. CI cannot catch the alternative: with no window open the
  resolution falls through to "none running" and the test is green on exactly
  the machine where the bug cannot happen.
- If tabs replace the accordion, `shelf()` may become the wrong shape. Change it
  deliberately rather than routing around it.
- The terminal-delight **MCP server was down all session**, so no pane census
  was possible; peer identification was done from git and `/proc`.

## Where it's recorded

- APES episode: `apes/projects/terminal-delight/episodes/2026-09-18-attention-is-a-tier-a-thing-is-handed.md`
- Session harvest: `handoffs/2026-09-18-bench-attention-and-surface-race.cdx`
- lean-ctx: `ctx_session` decision breadcrumb (ctx_knowledge was not bound)
- file-memory: `a-test-that-drives-a-ui-verb-aims-at-nothing`,
  `unique-by-construction-not-by-luck`, `the-bench-has-one-attention-resolver`,
  `artifacts-for-parker-are-commentable`, `gpui-kit-is-the-reference-implementation`
- Rescued material: `refs/attempts/rescued-2026-09-18/*` and
  `~/Work/reports/rescued-2026-09-18/`
- Issues: #550, #551 (open) · #552 (closed)
