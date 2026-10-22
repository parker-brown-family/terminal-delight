# Status: Clean, and the UI CONFIG it needs

**Difficulty: 7/10** — Clean stopped being one design file when Parker's notes reached "literally almost everywhere": every piece of chrome becomes hideable, at the window and for all panes, with a CONFIG menu that appears when the menu glyphs go, option rows that fold to their selection, and a new-window default somebody can change. It reaches many surfaces, and a hidden signal (a bell, a bug dot, a waiting card) can stay wrong for a long time before anyone notices. → **four gates**.
**Turned out to be:** _(filled in at the end)_

- Gate 1 — Product: APPROVED 2026-10-22 with changes — `reports/2026-10-21-ui-config-workshop.html`, notes saved in it; the changes are in `02-build-plan.md`
- Gates 2–4 — collapsed into `02-build-plan.md` on Parker's instruction ("maybe you JUST SMASH IT!!!! Make it award winning!"); building on `feat/clean-ui-config` from dev/0.5.0, worktree `~/Work/td-authority-lab/clean-ui-config` (it needs that lab's patched zed-upstream: the shared one lacks the image-glow patches)

## Slices
- [x] 1 — tracer: Clean design, Quiet retired, light preset, tube and light holds, new windows wear Clean
- [x] 2 — UI set layer, outer gates, CONFIG
- [x] 3 — pane header gates, ⋯ order, waiting state word
- [x] 4 — UI CONFIG in the painter palette
- [x] 5 — nearly square (corner_cap, clean skin); selection is the existing single border
- [x] 6 — folding rows (DESIGN, SKIN, PROGRAM COLOUR, SYNTAX; THEME and sticky note not yet)
- [x] 7 — nine languages, `terminal-delight ui`, docs/ui-sets, tests, gates; build on workspace 7

Built 2026-10-22 as commit 2058da52 on `feat/clean-ui-config`, draft PR https://github.com/parker-brown-family/terminal-delight/pull/1098 (base dev/0.5.0, milestone 0.5.0). A fresh-install copy runs on workspace 7: `sh ~/.cache/clean-play/launch.sh` (own TD_SESSION `clean-play`, own XDG_CONFIG_HOME under ~/.cache/clean-play/config). Waiting on Parker's hands-on feedback.

## Settled before Gate 1 — the Clean workshop, Parker's notes of 2026-10-21

The first workshop is `reports/2026-10-21-clean-design-workshop.html`; his notes are saved against it. What they decided:

1. **Clean reaches rung C, plus near-square corners.** No tube texture, no glow on letters, no bloom on cards or the frame, no light on borders and menus, no afterglow. Corners reduced "A BUNCH, not perfectly square, but close".
2. **Clean holds the tube off while it is worn** ("okay"). The CRT switch keeps its setting.
3. **"CLEAN BULLDOZES QUIET!"** Quiet goes; a saved Quiet reads back as Clean.
4. **New-window settings are customizable, and their default is Clean.**
5. **The default install settings get re-tuned later** ("we will checkin and RE-TUNE our default install settings").
6. **A selected option is never drawn with a double border** — "simpler than that".
7. **Option rows collapse to the selected option**, and a 900 ms hover opens them.
8. **Chrome visibility is configurable in its own UI CONFIG section of the painter palette**, with an OUTER layer and a PANE layer. The PANE layer is one global config for all panes: no pane is in charge of its own. The bottom-right menu glyphs hide individually or as a group; when they are hidden, a classic CONFIG dropdown (like an old File menu) takes their place. TERM | BENCH and the artifacts dropdown are part of it.

## Notes for a fresh session
- Parker's notes on any brief come back as an anchored map; `document_notes` reads them when the brief is open beside this pane.
- The running build is the 0.5 daily (`td-3ff49b8e-0.5-restricted`); read UI code from that commit, not from the primary checkout's branch, which predates Intrusive.
- Work lands on the 0.5 development branch.
