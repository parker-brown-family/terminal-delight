# Handoff — the response feed, and the transport that carried nothing (2026-09-18)

## Status

**All landed, merged and installed.** Three pull requests merged: #504 (the
response feed and both launcher fixes), #506 (title-card vitals and the corrected
tool description), #518 (the fence reader, the transport ranking, the compact
fold). Main is `f48166c`, built and installed as
`~/.local/lib/terminal-delight/td-f48166c-main`, and the session-1 window was cut
over to it with **24 panes intact**.

Nothing uncommitted. This worktree (`~/Work/td-response`) is on a **detached
HEAD** at `f48166c` — deliberate, it was detached to build main. The branches
`workbench/response-overview`, `bench/title-card-vitals` and
`bench/fence-and-transports` are all merged and can be deleted.

## What's done

| Change | Verified by |
|---|---|
| TDSP 0.3 `response` kind — required `tldr`, six known registers with aliases, any other key kept as a section, shape-driven bodies, doubts with per-doubt confidence | parser tests; `the_catalogue_and_the_parser_cannot_disagree` |
| OVERVIEW holds responses only; changeset → decisions; unclassified → artifacts | `every_kind_files_under_exactly_one_shelf` rewritten to assert each shelf holds only its own |
| Newest response stands in on the overview with nothing opened; an opened card survives arrivals | `the_overview_shows_the_newest_reply_until_a_person_opens_another` |
| Folding as bench state, toggles not open-states, forgotten on retire | `a_section_starts_at_its_default_and_a_toggle_flips_it_and_only_it` |
| Launcher effort = the harness's own `--effort` / `model_reasoning_effort` levels, clamped across a harness switch | `the_chip_word_is_the_flag_word_for_every_level_on_every_harness` |
| `launcher::panel_height` sums the real chrome; scan keeps all 257 dirs | `the_panel_leaves_the_rows_their_own_room_above_the_chrome` |
| Title card carries the turn's clock, tokens and in-flight tool; unread ≠ zero | `turn_vitals_exist_only_while_working_and_an_unread_screen_says_so` |
| **A `td` fence an agent prints becomes a surface** | five `derive` tests; **proved by removing the fix and watching three fail** |
| The briefing ranks its transports, fence marked `LAST RESORT` | `the_briefing_ranks_the_transports_and_puts_the_fence_last`, which reads their positions in the generated text |
| A compact response keeps its fold | `an_interactive_kind_has_one_renderer_for_both_sizes`, a comment-stripped source scan; **proved by replacing the delegation** |

**1246 tests, clippy `-D warnings`, rustfmt — all clean.** Photographed on a live
agent: two response cards on the bench with **zero** JSON files in the pane's
directory, so both arrived through the fence path alone.

## How to run / verify

```bash
cd /home/parker/Work/td-response/app && cargo fmt -- --check && cargo clippy --locked -- -D warnings && cargo test --locked
```

To watch the whole loop on a throwaway instance:

```bash
setsid env -u TD_PANE_ID -u TD_CUTOVER_GRACE -u TD_TAG TD_SESSION=wbtry TD_MCP=1 TD_MCP_WRITE=1 ~/.local/bin/terminal-delight &
```

It launches a briefed agent by itself. Then send it a word and flip to BENCH:

```bash
~/.local/bin/terminal-delight ctl --pid <window-pid> bench say 'test'
```

## Not done / next

- **#519** every transport `catalogue()` advertises needs a test that drives it
  the way the advertisement describes. This is the generalisation of the day's
  real bug and the highest-value one.
- **#520** `Origin::Derived` now means both *observed off a screen* and *declared
  as a fence*, and a reader cannot tell which.
- **#521** `~/.local/lib/terminal-delight/` is 4.0 GB across 91 builds, 1.9 GB of
  it referenced by no running process and older than three days.
- **#505** the visual check — largely satisfied now; photographs are in
  `~/Work/reports/wbresponse-2026-09-18/`. Close it or narrow it to the
  compact-width case.

## Watch out

- **The lean-ctx root jail is pinned to the SessionStart cwd.** Every `ctx_read`
  in a sibling worktree is refused; this repo has 23 worktrees. Use native `Read`
  there, or start the session in the worktree.
- **`cargo fmt` invalidates every prior Read**, so the next `Edit` fails. Batch
  edits, fmt once at the end.
- **A pull request can be merged out from under you.** #506 was merged by another
  session while two commits were still being pushed to its branch; they were not
  in main and `gh pr merge` answered *"already merged"*, which reads like success.
  Check `git merge-base --is-ancestor <sha> origin/main`.
- **`~/.local/bin/terminal-delight` is contested** — it was repointed twice by
  other agents mid-session, so a test rig silently ran somebody else's binary.
  Read `/proc/<pid>/exe` of any rig before believing what it shows.
- **Displays asleep = no screenshots.** `grim` hangs to its timeout when
  `dpmsStatus` is false, and Hyprland 0.56 is the Lua compositor, so
  `hyprctl dispatch dpms on` is a syntax error.

## Where it's recorded

- APES episode: `apes/projects/terminal-delight/episodes/2026-09-18-the-transport-that-carried-nothing.md`
- APES kanban: three follow-up tickets, cross-linked to #519/#520/#521
- Session harvest: `handoffs/2026-09-18-response-feed-and-transports.cdx`
- lean-ctx: session decision recorded (`ctx_knowledge` was not bound this session)
- File memory: `an-advertised-capability-needs-an-end-to-end-gate`,
  `a-pr-can-merge-out-from-under-you`, `the-root-jail-follows-the-cwd-not-the-repo`,
  `a-sleeping-output-cannot-be-photographed`
- Findings page: `~/Work/reports/wbresponse-2026-09-18/FINDINGS.md`
