# Handoff — workbench provenance (2026-09-17)

## Status
Landed on `workbench/provenance` — five commits `de0952f..27de865`, **pushed**, tree clean.
Stacked on `workbench/surface-protocol` at `1ebad62`, which is **not on origin**, so no PR
exists yet. Not installed. Nothing clicked or photographed under an opened card.

## What's done
| Move | Commit | Verified by |
|---|---|---|
| 3 · bench signs what it types (`TD_TAG`, `[workbench:<tag>]`, one-line scrub, briefing/spec/snippet) | `de0952f` | gates green; scrub test fails without the scrub; host test asserts `TD_TAG=` in the child's environ; apply no-op test |
| 1 · show the bytes under every verb | `05b92bc` | `verb_preview` == `to_prompt` for a newline hunk id; fails when the tag is dropped |
| 2 · origin on every card | `791f3dc` | a file drop claiming `origin`/`writer` still reads `writer unknown`; fails when the sweep forgets to stamp |
| 4 · on screen or not at all, `Reading your answer`, socket verbs answer on outcome | `65491f1` | pure `agent_state` table; fails when reading stops outranking asking; smoke run's refusal steps assert the refusal text |
| 5 · reach dial + launch journal | `27de865` | `Anywhere` produces the unchanged line, fails under mutation; flags read off `claude --help` 2.1.270 / `codex --help` 0.151.0 |

Gates at HEAD: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --locked` → 1,164 + 4 + 8 + 8 passed, 0 failed, 6 ignored (pre-existing).

## How to run / verify
```
cd ~/Work/td-provenance/app && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test --locked
```
```
cd ~/Work/td-provenance/app && cargo build --release
```
The five-minute test, by a person: open a card → read the preview lines under the verbs → press one → the agent bar turns to *Reading your answer* and the card edge flashes → from another tab, in a shell:
```
terminal-delight ctl --pid <window pid> bench say hi
```
expect `queued pane N — off screen until you look`; switch back → it lands, the header badge clears.

## Not done / next
- **Open the PR** against `workbench/surface-protocol` once that branch is on origin. Body: `handoffs/../` — see the tie-off scratchpad `pr-body.md` or rewrite from this table.
- **Photograph** the verb preview and the reach row — needs #492 (`ctl bench open`, `ctl bench launcher`) and #491 (the smoke window's geometry).
- #483 claimed-writer field (pinned), #484 multiplayer (funded, one day).
- The derived sweep's cost (256 KiB per agent pane per second, no stamp) is untouched — its own slice.
- Install the AGENTS.md paragraph (`docs/spec/agents-md-snippet.md`) **after** this branch is installed, not before: today's paragraph on an old build would teach the forgeable phrase.

## Watch out
- `to_prompt` now takes `tag: Option<&str>`; `bench_choose/bench_say/bench_type` on the workspace return `String` outcomes and `Req::Bench*` carry an `mpsc::Sender` — any new caller must thread the reply.
- The tag file: `$XDG_RUNTIME_DIR/terminal-delight/session-<key>.tag`, `0600`. The host test leaves `session-session-under-test.tag` behind; harmless.
- Peers are active on the base branch's worktree (`~/Work/td-workbench`) with uncommitted edits — do not work there. Rebase this branch before the PR; the last rebase was clean.
- Running `cargo fmt` (apply) inside a gates job invalidates every prior `Read` for the Edit tool — re-read before editing, or run `fmt --check`.
- A grim shot by geometry is whatever sits in that rectangle (#491); check the pane header before reading a shot.

## Where it's recorded
- APES episode: `apes/projects/terminal-delight/episodes/2026-09-17-cowboy-on-authority-engineer-on-provenance.md` (+ `.cdx` harvest beside it)
- Briefs: `~/Work/reports/2026-09-17-workbench-adversarial-review.html`, `~/Work/reports/2026-09-17-workbench-security-posture.html`
- Memory: `the-tag-lives-beside-the-socket`, `a-remark-in-passing-reaches-two-agents`, `a-region-shot-is-whatever-is-in-the-region`
- Issues: #483, #484, #491, #492
