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

## Native delivery

The isolated release binary is `/home/parker/Work/td-phosphor-glow-lab/bin/terminal-delight-phosphor`, SHA-256 `fb145f11e537e9332f6e9bd4ee587f34bab30f361aab49f7d7ca70c39b79ccf0`. Session `phosphor-glow-test` runs on workspace 4 with interactive Bash panes: 55% inherited glow on the left and explicit 0% pinned on the right. Its settings are private; ordinary session 1 remains running. Relaunch with `sh /home/parker/Work/td-phosphor-glow-lab/native-lab/launch.sh`.

Default core: 2,594 tests passed, 10 ignored. Alacritty core: 2,541 passed, 10 ignored. Both Clippy gates, format, locked release build and fresh patch replay passed. Native grid comparisons prove exact restoration at zero and CRT off, no change to the reference pane, and persistence after a restart. GAUGES still suppresses CRT preview while open; close it to see the dial. GPU frame cost remains unmeasured.

Review: https://github.com/parker-brown-family/terminal-delight/pull/1074 (draft, base main, no release milestone). GPU timing qualification: https://github.com/parker-brown-family/terminal-delight/issues/1075. Native delivery is complete; Parker’s strength/readability judgement is pending.
