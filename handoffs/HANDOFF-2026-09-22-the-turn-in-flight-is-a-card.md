# Handoff — the turn in flight is a card (2026-09-22)

## Status

**Merged.** PR #672 merged 16:35:53Z (`68db195`), carrying PR #659's two commits
as ancestors after being retargeted to `main`; #659 merged two seconds later.
The merge commit `e2e1cab` is an ancestor of `origin/main`. Nothing uncommitted.
The local branch ref was deleted on merge; this worktree is on `main`.

**Parker has still not seen it on his own screen** — see *Watch out*.

## What's done

The workbench overview's room used to hold the newest `response` surface, and a
response is presented at the END of a turn — so for the whole length of every
turn it held the *previous* turn's answer under the current turn's question.

`Bench::live: Option<LiveTurn>` is a turn that has begun and presented nothing.
It takes the overview's head row (`THIS TURN`) and the room, drawn as the
agent's own gerund, the one tool call it is inside, its clock and its tokens,
under the person's own message. It is **not a surface** — nothing entered the
store, nothing is retired, its row cannot be opened.

| verified by | what |
|---|---|
| tests | 1583 pass; `clippy --locked -- -D warnings` and `fmt --check` clean |
| mutation | **18 of 18 caught**, on a harness first proved to report SURVIVED, CAUGHT and BUILD ERROR |
| photograph | the *installed* binary drawing the card, on a throwaway window |

Two defects were found and **neither by a test**:

1. **The card was pinned to the floor of the pane** under an acre of empty —
   `body_anchor` was keyed on `showing_id.is_some()`, a proxy for *is a card in
   the room* that stopped being true when a card that is not a surface arrived.
   Found by looking at a photograph.
2. **The at-rest card predicted the future on every turn** — `NOTHING PRESENTED
   · this turn ended without presenting a reply`, drawn in the race between the
   finish bell (120ms effects clock) and the harness's stop hook. Found by
   re-reading my own words after Parker had already approved the decision.

## How to run / verify

```bash
cd /home/parker/Work/terminal-delight/app && cargo fmt --check && cargo clippy --locked -- -D warnings && cargo test --locked
```

Two `paneident` tests fail on this box and are environmental — they spawn a child
and compare pids, and with six real agents running they find one of those
instead. Neither file is in this diff.

Photograph it (unfocused monitor, empty workspace, restores focus on exit):

```bash
TD_BIN=/home/parker/.local/bin/terminal-delight bash ~/Work/reports/live-turn-2026-09-21/shoot-live-turn.sh
```

## Not done / next

- **Parker has not seen it.** A build of current `main` plus a **window bounce**
  is what shows it. See *Watch out*.
- `terminal-delight#692` — a window never says it is running an older build than
  the one installed. APES: `a-window-should-say-when-it-is-running-an-older-build-…-mucz20ye`.
- `context-delight#22` — `cdx-audit`'s `duplicate-action` fires on a verification
  loop. APES: `cdx-audit-duplicate-action-fires-on-a-verification-loop-mucz2d6i`.
- `terminal-delight#586` — the photograph rig still lives only in a reports
  directory; it wants to be `scripts/fake-agent`. The working recipe is now a
  comment on that issue.
- **Decision 2 is parked, not settled.** Parker: *"gets tricky and grey - but
  let's leave it in assumption land right now."* The rule is that a reply retires
  the card and going idle does not. Its wording was amended the same sitting; the
  rule itself was not.

## Watch out

- **A session restart is NOT a window restart.** Restarting an agent session
  re-attaches to the running host and keeps the binary the WINDOW launched with.
  Only bouncing the window picks up a new build. This cost the better part of an
  exchange today.
- **The launcher is contested.** It was repointed three times in one session by
  three agents — `td-2d46e72-fast-scan` (09:04), `td-e2e1cab-live-turn` (mine,
  09:26), `td-eff2abc-qround2` (10:48) — while every running window sat on
  `td-7d41cb5-main` throughout. Installing is a courtesy that survives until the
  next agent; **merging is the durable answer**, and that is done.
- **`main` is 12 commits past the merge.** Build `main`, not this branch.
- **The worktree is shared.** It moved from my branch to `main` under me
  mid-tie-off. Check `git branch --show-current` before assuming anything, and
  grep for a marker only your change introduced before concluding it survived.
- **`gh pr merge` is refused by the harness** ("Merge Without Review"). Merging
  is Parker's.

## Where it's recorded

- APES episode — `apes/projects/terminal-delight/episodes/2026-09-22-the-turn-in-flight-is-a-card.md`
  (+ the `.cdx` beside it)
- Plan page — `docs/plans/the-turn-in-flight-is-a-card/00-status.md`
- Report page — `~/Work/reports/live-turn-2026-09-21/2026-09-21-the-turn-in-flight-is-a-card.md`
- Transcript package — `handoffs/2026-09-22-the-turn-in-flight-is-a-card.cdx`
- lean-ctx — 4 findings + the session decision (`ctx_knowledge` did not bind)
- file-memory — `merged-is-not-installed` and
  `a-proxy-predicate-goes-stale-when-a-third-case-arrives` extended;
  `an-at-rest-message-must-not-predict` and
  `a-long-wait-becomes-a-job-you-then-poll` added
- PRs — #672, #659
