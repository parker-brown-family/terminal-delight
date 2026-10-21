# Status: Image Glow

**Difficulty: 6/10** — one appearance control reaches a GPU post-process and must keep its zero state exact.
**Turned out to be: 6/10.** GPU isolation, exact zero comparisons and legacy warp migration in the fixture justified the initial score.

- Combined plan: approved to proceed by Parker's 2026-10-20 instruction, “afk so do it end to end and give me a TD instance with it built for myh testing!” This overrides interactive approval stops.
- Branch: `feat/crt-phosphor-glow`, from `origin/main` at `70e0d00de9a1449421608716243b7839250acf2a`.
- Build isolation: this worktree and a private patched GPUI worktree share `/home/parker/Work/td-phosphor-glow-lab/`; the ordinary desktop build uses neither.
- Release intent: unassigned. The agent communication milestone does not include this appearance experiment automatically.

## Slices

- [x] Add the image-glow grade, CRT slider, saved settings and MCP bridge.
- [x] Implement bounded image bloom in the existing GPU pass, including flat tubes.
- [x] Verify zero/off, per-pane isolation, persistence, colours and native output.
- [x] Commit and publish the branch; leave a separate native test instance and a human review page.

## Resume

Parker accepted the cool-retro-term study and requested a dedicated effect branch with a slider. The first effect is terminal-image bloom. Independently implement it; upstream effect code is GPL-3.0-or-later. Keep unset distinct from an explicit zero. The existing GAUGES preview limitation is tracked in the glass-preview issue; record what this branch actually proves about it.

## Initial bloom-only delivery

The isolated release binary is `/home/parker/Work/td-phosphor-glow-lab/bin/terminal-delight-phosphor`, SHA-256 `fb145f11e537e9332f6e9bd4ee587f34bab30f361aab49f7d7ca70c39b79ccf0`. Session `phosphor-glow-test` runs on workspace 4 with interactive Bash panes: 55% inherited glow on the left and explicit 0% pinned on the right. Its settings are private; ordinary session 1 remains running. Relaunch with `sh /home/parker/Work/td-phosphor-glow-lab/native-lab/launch.sh`.

Default core: 2,594 tests passed, 10 ignored. Alacritty core: 2,541 passed, 10 ignored. Both Clippy gates, format, locked release build and fresh patch replay passed. Native grid comparisons prove exact restoration at zero and CRT off, no change to the reference pane, and persistence after a restart. GAUGES still suppresses CRT preview while open; close it to see the dial. GPU frame cost remains unmeasured.

Review: https://github.com/parker-brown-family/terminal-delight/pull/1074 (draft, base main, no release milestone). GPU timing qualification: https://github.com/parker-brown-family/terminal-delight/issues/1075. Native delivery is complete; Parker’s strength/readability judgement is pending.

## Focus and afterglow phase

Difficulty: 7/10. Parker expanded the request on 2026-10-21: active 15% glow, inactive 65% glow with lower text intensity, real fading text and organised gauges. The four design stages are recorded in 01-focus-product.md through 04-focus-slices.md under the continuing end-to-end authorisation; no separate human gate reviews are claimed. Preserve existing picture grades. Inactive text starts at 75% pending Parker’s optional preference.

- [x] Focus defaults, fields, MCP and grouped gauges.
- [x] Bounded per-pane glyph afterglow.
- [x] Native focus/fade checks and refreshed test instance.
- [x] Same PR, human review and qualification record updated.

The current test GUI is PID 2104454 on workspace 4, session `phosphor-glow-test`, using binary SHA-256 `209beed8441fd9fe940b234f2cd6b488fb70ec158b48db3ca18ee0d26c92d7d6`. Its active `TERMINAL LIGHT` tab has two fresh interactive shells. The original hosted shells and their typed text are preserved in the earlier tab; no PTY input was sent to them. The lab uses OPS and the captured picture grade, without changing ordinary session 1.

Defaults resolve at drawing time to active glow 15%, inactive glow 65%, inactive text 75% of the active text setting, and 180ms of replaced-glyph afterglow. All four lighting fields stay optional in storage and GET reports, so explicit zero survives independently. Letter glow and afterglow are independent of the CRT glass switch. This deliberately supersedes the first build's CRT-off bloom contract.

Current verification: default core 2,606 passed, fallback 2,553 passed, 10 ignored in each. Both Clippy gates, format, locked release build and native fade/default/zero/pane-isolation checks passed. Two new tests initially exposed float comparison and duration rounding errors; both were corrected before the passing full runs. The retained trail is capped at 4,096 glyphs per pane, expires on clock time and clears when its drawing context changes. No GPU history texture was added. Frame cost remains unmeasured and stays in the existing qualification issue.

Turned out to be 7/10: keeping fading glyph positions correct across focus, geometry and terminal-face changes required a separate retained drawing state and native timing evidence. Individual design gates were recorded before code under the continuing end-to-end instruction; no independent human approvals are implied.

The current human review is `reports/2026-10-21-terminal-light.html`, with native receipts, two restart checks, pixel comparisons, test counts and page QA in `reports/terminal-light-proof/`. Native restarts preserved explicit zero and the inactive-glow, inactive-text and afterglow pins independently; no seed was rewritten. Reader controls, figure/decision annotation coverage, concurrence, map export, embedded save and reopen passed at 390/968/1100px. The narrative measurement classified the page as an argument and scored cold comprehension 0.78; the CLI instructions still carry names by necessity. The same draft pull request and frame-cost qualification were updated; release intent remains unassigned. Parker's tuning judgement and promotion qualification are pending, while the requested testing build is delivered.
