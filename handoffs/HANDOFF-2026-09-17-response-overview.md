# Handoff · 2026-09-17 · the response feed, and the launcher's two bugs

**Branch** `workbench/response-overview` · **PR** [#504](https://github.com/parker-brown-family/terminal-delight/pull/504) · **follow-up** [#505](https://github.com/parker-brown-family/terminal-delight/issues/505)
**Worktree** `~/Work/td-response` (made off `origin/main` at `aa37d5d`, because `~/Work/terminal-delight` sat on `spine/instance-identity` without the launcher or the workbench)
**Installed** `~/.local/bin/terminal-delight → ~/.local/lib/terminal-delight/td-45867d8-response-overview` (was `td-70226ea-main`; flip back with one `ln -sfn`). A window already open keeps the old binary until it is relaunched.

## What Parker asked, in his words

1. *"effort does not correlate with ACTUAL claude efforts"* — the launcher's EFFORT row.
2. *"I don't see terminal delight"* — `ter` typed into the launcher, the PROJECT area empty.
3. The Overview as a feed of agent replies in a *"WELL DEFINED but VERY FLEXIBLE"* JSON — tl;dr, ELI5, technical brief, layman brief, articles of doubt, other ideas — as collapsible elements, with artifacts and decisions no longer on that tab. *"Let's make it award winning!"*

## What was found

- **Effort.** Claude Code 2.1.270 takes `--effort <low|medium|high|xhigh|max>`. The launcher's module comment said no such flag existed and appended a "think hard" sentence to the system prompt instead. Codex takes `model_reasoning_effort` with `low`, `medium`, `high`, `xhigh` (read off the binary's strings — one of the declared doubts).
- **The empty PROJECT area was a height bug, not a search bug.** Panel height was `250 + 30 per matched row`; 250 was the chrome when the panel had one chip row, and it has had four plus a command preview plus a key hint. Three matches → a 340-pixel panel → the list, the only child able to shrink, got zero height. The screenshot's panel measures ≈340 real pixels, which is the arithmetic. Behind it a second trap: the scan kept the 60 newest of 257 project directories, so a repo with no top-level change in a week fell off the picker.

## What was built

| Concern | Where | Proof |
|---|---|---|
| Effort chips = the harness's own levels, passed as the flag; clamp across a harness switch; no effort prose in the briefing | `app/src/launcher.rs`, `app/src/main.rs` | tests walk every level on every harness; briefing identical at every level |
| Panel height as a named sum of the chrome the render draws | `launcher::panel_height`, `PANEL_CHROME_H` | test holds rows-above-chrome for every count to the cap, and that 250 would not |
| Scan keeps every directory (cap 1000), render caps rows at 40 | `main.rs` `open_agent_launcher` | |
| TDSP 0.3 `response`: `Response`, `Section`, `Register`, `Body`, `Doubt`; aliases; flat or nested under `model.response`; shape-driven bodies | `app/src/surface.rs` | parse tests; catalogue/parser agreement test |
| Overview holds responses only; changeset → decisions; unclassified → artifacts | `surface::Kind::shelf`, `Shelf::holds` | every-kind-one-shelf test rewritten |
| Folding: `Hit::ToggleSection`, toggles per surface forgotten on retire; default-open policy in `workbench::section_default_open` (only `asks`) | `app/src/workbench.rs`, `app/src/pane/bench.rs` | toggle test |
| `Bench::showing`: newest response stands in on the overview when nothing is opened; an opened card survives arrivals | `workbench.rs` | showing test |
| Renderer: gist block, per-section panels with clickable headers and measures, doubts strip with per-doubt confidence, compact and summary bodies | `app/src/benchdraw.rs` | the no-decisions guard still passes |
| Demo fixture carries a response | `app/src/surfacefeed.rs` | every-shelf-covered test |
| Spec §5 `response`, shelf table, 0.3 changelog; AGENTS.md snippet; CHANGELOG; plan page | `docs/spec/*`, `CHANGELOG.md`, `docs/plans/response-overview/00-status.md` | |
| Machine-global agents file told to end every turn with a response | `~/.config/agents/AGENTS.md` (outside the repo) | |

`cargo test --locked` 1246 passed · `clippy -D warnings` clean · `fmt --check` clean · release build installed.

## What is NOT done — read this first

**Nothing has been looked at on a screen.** A demo window was staged (`TD_WORKBENCH_DEMO=1`, seeded 7, `ctl bench on` → ok), but both outputs were DPMS-off and every `grim` hung to its timeout; `hl.dsp.dpms(...)` answered `ok` and woke nothing. The response card and the launcher panel are proved by tests and arithmetic only. That is [#505](https://github.com/parker-brown-family/terminal-delight/issues/505), with the exact recipe (`scripts/workbench-smoke.sh`) and the invalidation criterion. **Run it before merging #504**, or merge and run it after — but run it.

## Decisions taken without asking (argue with any)

- Registers live flat under `model`; the nested `model.response` Parker drew is accepted too.
- Known keys and labels: `tldr` (required) · `eli5` ELI5 · `layman` Plain brief · `technical` Technical brief · `evidence` What was verified · `asks` Needs from you · `next` What's next · `doubts` its own strip. Unknown keys kept, labelled by key, sorted after.
- Only `asks` starts open; gist and doubts are never folded.
- The newest response stands in on the overview by itself. An arrival never takes the room from an opened card.
- Default effort `high` (Claude) / `medium` (Codex); the flag is always printed.

## Traps met

- `~/Work/terminal-delight` was on `spine/instance-identity` and had none of this code — the ask names a surface, so check `main..HEAD` before concluding a feature is unbuilt. A worktree off `origin/main` was the fix.
- lean-ctx refuses paths outside `~/Work/terminal-delight`, and the shell hook blocks `sed -n`/`cat` on them too; native `Read` and `grep` work.
- `command -v gh` resolves to mise's gh; the wrapper is `~/bin/gh`, called explicitly.
- Hyprland 0.56 is the Lua compositor: `hyprctl dispatch dpms on` is a syntax error; it is `hyprctl eval 'hl.dsp.dpms({...})'`, and with the outputs asleep that is not enough to photograph anything.
