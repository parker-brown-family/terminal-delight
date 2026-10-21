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

## Deleted-text phase

Difficulty: 7/10. Parker's 2026-10-21 native review approves the grouping, sets Standard/Intrusive defaults from two screenshots, raises normal afterglow to 220ms and requests whole-passage erasures to linger roughly 450ms. The four design stages are recorded in 05-deleted-text-product.md through 08-deleted-text-slices.md under the continuing end-to-end instruction.

- [x] Complete erased passages survive bottom alignment and cursor movement.
- [x] Approved Standard/Intrusive lighting and shared picture defaults.
- [x] Native timing evidence, both cores/lints and refreshed test window.
- [x] Same PR, review and qualification record updated.

The refreshed test GUI is PID 4073052 on workspace 4, session `phosphor-glow-test`, using binary SHA-256 `81eb231005af6354ed33645988a517ef646549f4ebdad566ed22cd245e9a4a06`. The `ERASED TEXT · 220 / 450 ms` tab has fresh Standard and Intrusive shells. All four earlier hosted shell processes remain, with no PTY input sent to them. Both presets inherit the existing picture grade, palette and OPS skin; the lab's optional lighting pins were backed up and cleared to adopt the requested defaults.

Standard draws undeclared active glow at 20% and inactive text at 45% of the active setting; Intrusive uses 37% and 52%. Both draw inactive glow at 100%. Ordinary replacement defaults to 220ms. An update that loses visible graphemes on any painted row retains its changed old glyphs for at least 450ms, without waiting for input or restarting the clock when the cursor moves. Zero disables the trail. The history budget is the larger of 4,096 glyphs and one visible viewport, so a complete visible paragraph is not truncated by the old fixed cap. Genuine scroll, geometry, focus and theme changes still clear history; bottom alignment alone does not.

Default core: 2,612 passed; fallback: 2,559 passed; 10 ignored in each. Both strict Clippy gates, format and locked release build passed. Native captures retain both the word and wrapped paragraph after the cursor moves at about 60ms and capture begins around 258ms; the neighboring text region changes by zero pixels. Ordinary replacement is within one 8-bit RGB level of the settled frame after 220ms; disabled erasure matches exactly. Deterministic tests verify exact 450ms expiry and retained glyph coordinates.

The larger 107% fresh text default exposed the native harness's old 640px-high window clipping its intentional 100×28 grid. Increasing only the fixture height to 720px restores the original grid and all interaction assertions. Compatibility defaults for fields omitted from old grades remain unchanged; the screenshot's new picture values apply through `house_outer`, not through deserialization fallbacks.

The current review is `reports/2026-10-21-deleted-text-afterglow.html`; its receipts are in `reports/deleted-text-afterglow-proof/`. All six recorded paragraph/word captures, responsive layout at 390/968/1100px, figure and decision annotations, concurrence, map export, embedded save and reopen passed. The narrative check scored opening comprehension 0.88 and classified the page as an argument. Source and commands in the expandable verification section still carry identifiers. The same draft PR and existing frame-cost qualification issue reflect this phase. Remote Rust CI is pending; all local gates passed. No merge or release assignment was made.

Turned out to be 7/10: deletion exposed a painted-coordinate invalidation, a viewport-sized history requirement and a classifier interaction between independently changing rows. Native evidence found the row-count cancellation that pure replacement tests had missed. The testing instance and review are delivered; Parker's visual judgement and the existing promotion qualification remain open.
