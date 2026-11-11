# Make the state machines explicit, and check every one

**Difficulty: 8/10.** These changes touch how every overlay, key and pane rest behaves. A wrong one can sit unnoticed for days, and most of the code is in `main.rs`. That score buys all four gates, but Parker asked for one plan to finish today, so this page is the single gate and the only approval.

**Status:** rows 1–5 shipped to dev/0.5.1 on 2026-11-10; rows 6–8 being built on 2026-11-11 for the 0.5.3 launch (Parker: *"they should have been ALSO built"*)

## What "done" means for each ask

| Ask | What the night delivered | What is still missing |
|---|---|---|
| 1. Audit, with a before and after for each, in a brief | All of it: ten machines, each drawn before and after | Nothing. Making the "after" real in code is new scope, added today |
| 2. A model and skill per subsystem; charts of the main user flows | 4 of 10 subsystems modelled, plus the INBOX and Settings book flows | Models for drags, keys inside a pane, side-panel loading and the composer; charts of three everyday flows |

## The one idea that holds it together

**Every subsystem follows the same loop: model → change the code → the model's findings list empties.**

- The model is written first, from the code as it stands.
- Each rule the code breaks today goes into `findings.txt`. That file is the to-do list.
- The code changes until `verify/check.sh` reports `FIXED?` for every entry. Then the entry is deleted.
- A window test replays each finding on the real window. It fails before the change and passes after, so the proof covers the real window and not only the model.
- An empty `findings.txt` means that subsystem is finished. There is no other definition of done, so nothing has to be judged by eye.

## The order

The work is ordered so that harm comes out early, and so that finished pieces are never left half-built when the day ends.

| # | Subsystem (section in the brief) | Model / skill | Code change (the "after") | Proof | Est. |
|---|---|---|---|---|---|
| 1 | Side-panel tab loading (08) | new | `Reading` holds one state: waiting, ready or failed, so a result and an error can no longer coexist | the model and unit tests | 1 h |
| 2 | Composer faces (09) | new | one `face` field; Esc, Tab and Enter each read one table | model + window test | 1 h |
| 3 | Worktree removal (04) | exists | move the tree to a quarantine name, re-check, then remove | `findings` empties; a tree someone walks into survives | 1 h |
| 4 | Rest and sleep (03, issue 1176) | exists | the look records what it saw; freeze only if none of it moved; sleep refuses a pane with a pending timer | `NeverFrozenMidTurn` and `SleepLosesNothing` flip to holding; replay test | 2 h |
| 5 | What is on top (01) and the Settings book (07) | exists | one stack in raise order; drawing, keys and Esc all read it; a hosted panel is one more entry on it | all three overlay replays pass (`verify_overlay.rs` loses its `#[ignore]`) | 4 h |
| 6 | Drags (05) | new | one `Drag` enum replaces seventeen fields | model + test for a stuck drag | 2 h |
| 7 | Keys inside a pane (06) | new | one owner type replaces the order of the `if`s | model + keyboard skill re-run | 2 h |
| 8 | Three everyday flows | new charts | launching an agent; one agent prompting another across the Threshing Floor; an agent opening its document beside itself | TLC holds; charts drawn | 1.5 h |
| 9 | Closing brief | — | a new page (`--follows` last night's) that carries only what changed: findings before and after for each of the ten, plus the three flows | the brief's gates pass | 1 h |

**About 15 hours of agent time.** Rows 1–5 and 8–9, about 11 hours, are the tie-off line. Drags (row 6) and keys inside a pane (row 7) come last because they carry the least harm and the most churn. If the day runs out, they still get their models and findings, so ask 2 is complete either way. Their code change then goes into a falsifiable issue whose acceptance test is "findings.txt empties". Nothing is left half-built.

## How it lands

- One PR per row onto `dev/0.5.1`, milestone 0.5.3. Each has a three-paragraph commit and passes the gates before it merges: fmt, clippy `-D warnings`, the full suite in a capped scope, and a mutation test of the new rule.
- `verify/README.md` gains a table with one row per subsystem: the skill, the model, and the findings count, which should read 0 at the end.
- The swap onto dev still needs Parker to release panes 11, 25 and 26. Without that release, all of this is merged but not live.

## Calls I made, for Parker to stamp or overturn

1. **One agent, in order, no fan-out.** Rows 3–7 all edit `main.rs`. Parallel agents would spend their savings on merge conflicts. If speed matters more, the split that works is two agents: rows 1, 2 and 8, which barely touch `main.rs`, beside rows 3–7. That costs roughly two context loads instead of one.
2. **The model is the spec.** Where the brief's drawn "after" and the model disagree, the model wins, and the brief's next page says so.
3. **Section 02 (keyboard) and section 10 (INBOX) need no code.** Both already hold under their checks.
4. **No new brief until the end.** Progress shows in the PRs and in `findings.txt`. The single closing page is the thing to read.

## Progress (post-hoc)

| # | Row | State |
|---|---|---|
| 1 | Side-panel reading | PR 1419 merged: one `Shown` value plus the read in flight; a stale red note fixed |
| 2 | Composer | PR 1422 merged: PROJECT's rows derived from the query, never stored |
| 3 | Worktree removal | PR 1423 merged: quarantine name, then remove |
| 4 | Rest and sleep | PR 1421 merged: a pending wakeup refuses rest and sleep; a freeze is checked against the look (issue 1176) |
| 5 | Overlay stack | PR 1424 merged: a raised surface puts away the ones beneath it |
| 6 | Drags | model PR 1427; the `Drag` enum is issue 1425, built for 0.5.3 |
| 7 | Keys inside a pane | issue 1426, modelled and built for 0.5.3 |
| 8 | Three flows | issue 1432, charted for 0.5.3 |
| 9 | Closing brief | `reports/2026-11-10-state-machines-made-explicit.html` |

Also on 2026-11-10: dev's CI, red since PR 1391, went green with PR 1431, and the window was swapped onto dev at 018621e7.

**Difficulty, post-hoc:** 8/10 held. The single-plan approval was the right size: no gate came back as a bare approval, and the two departures from the brief (side-panel and overlay shapes) were decided by the models.
