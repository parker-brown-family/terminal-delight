# Status: GUI in the TUI — documents open in the pane

**Difficulty: 7/10** — it writes notes into Parker's brief files, changes four click gestures he uses every day, and the HTML engine is a fork he called "a REALLY HARD question"; a wrong call there stays expensive → **four gates**.
**Turned out to be:** _filled in at the end_

- Gate 1 — Product: **APPROVED 2026-09-24** — through Parker's notes on the research brief and his chat message the same day. `01-product.md` records those decisions; its success metric and announcement are mine and were not in his notes — strike them if wrong.
- Gate 2 — Architecture: **APPROVED 2026-09-24** — Parker: "LGTM --- LFG!" on `02-architecture.md` as drafted, before the snapshot spike reported. If the spike contradicts the notes-layer design (anchor ids that differ from a browser's, or an unsafe write), Gate 2 goes back to in progress.
- Gate 3 — Program Design: **APPROVED 2026-09-24** — Parker concurred with all six asks on `reports/2026-09-24-ten-slices.html` (his saved copy is the one in the repo) and wrote "full concur full send full alignment". No notes, nothing changed.
- Gate 4 — Slice plan: **APPROVED 2026-09-24** — same page, same concurs; `04-slices.md` as drafted.

## Slices
- [x] Slice 1 — tracer bullet: Alt+click a PNG, it floats in the hyperglow, Esc closes it, textures given back (counted over 100 cycles) — merged in PR #721 (b267379). Soak: `scripts/doc-float-soak.sh`, samples in `evidence/slice-1-soak-*.csv`; 158–160 MiB after every one of 100 closes, control without `drop_image` +64 MiB a cycle.
- [x] Slice 2 — the floating square, whole: drag, strip actions, bent-glass hits, the chip, the click table, the menu, zoom and pan, help and languages — merged in PR #723; installed as td-34a1e70-floating-square.
- [x] Slice 3 — Markdown: lifted renderer, TD palette, links, images, scroll, watcher, workbench cards — merged in PR #728 (comrak +8 packages; follow-ups #725, #726, #727).
- [x] Slice 4 — the split: Ctrl+Alt+click beside, focus kept, dedupe, four-pane fallback, promotion, alt+k, keys swallowed — merged in PR #729; `ctl doc beside`; installed as td-ffba91b-doc-split.
- [x] Slice 5 — the layout remembers: saved document panes, missing files, replica repair — merged in PR #730.
- [x] Slice 6 — HTML read-only: engine trait, pipe client, snapshot engine, tiles, cache, links and modals, missing-Chromium fallback — merged in PR #735; installed as td-240a89d-doc-html. Real-Chromium tests skip on CI until the workflow gains the AppArmor line (#737, needs the `workflow` token scope from Parker). Follow-ups #733, #734.
- [ ] Slice 7 (building with slice 8, worktree `~/Work/td-slice7-notes`) — notes read: badges, buttons, note box, concur stamps, copy map, shared fixtures (landed in agent-skills PR #29, 0eb9e20)
- [ ] Slice 8 — notes write: add, delete, concur, save in place with verify, backup and read-back, refusals, on-disk changes
- [x] Slice 9 — size answers: live 14 t (#718), replica filter, serverless 14 t, device pixels, 16 t scanner — merged in PR #720 (cf04ea1), closes #718; `device_cell` lives in `ptyscan.rs`. Follow-up found on the way: colour queries (OSC 10/11/12, OSC 4) go unanswered, #719.
- [ ] Slice 10 — guards: fixture drift watch, format version, licences, status closed

## Notes for a fresh session
- The build log for Parker is `reports/2026-09-24-floating-square.html` (assembler `reports/_assemble_floating_square.py`, chart drawn from `evidence/`). Update it as slices land rather than starting a new page per slice.
- Slices 2, 3 and 6 were built in parallel on 2026-09-24, then 4 and 5, then 7 and 8; each opened its own PR after rebasing on main.
- The notes format is pinned by agent-skills PR #29 (0eb9e20): `data-format="1"`, `data-rev` set like `setAttribute`, `</` → `<\/` and `<!` → `\u003c!` in the islands, the READER NOTES mirror replaced not appended, unknown formats refused. Fixtures: `decision-brief/fixtures/notes-format/` in agent-skills.
- To try it by hand: Ctrl+Alt+T for a fresh window from the launcher, then `scripts/doc-demo.sh`.
- Calibration: Gates 2, 3 and 4 all came back as bare approvals — nothing changed, nothing questioned. Only Gate 1 moved the plan (Parker's gestures and the notes requirement). Recorded as evidence that the 7/10 bought more gates than it needed; the post-hoc number goes in "Turned out to be" when the slices are done.
- The decision-brief notes format gained CONCUR stamps on 2026-09-24 (skill commit 250188f): a second island, `report-concurs`. TD's notes layer reads and writes both, guarded by shared round-trip fixtures.
- The research behind this is on the local branch `research/gui-in-the-tui` in `~/Work/td-gui-in-the-tui` (commits b26b67a, 4a2a6d1): the brief `reports/2026-09-24-gui-in-the-tui.html` and four write-ups under `docs/research/gui-in-the-tui/`. That branch quotes outside source and is not meant to merge; cite it by commit.
- Parker's annotated copy of the research brief, with his eleven notes, is `~/Downloads/2026-09-24-gui-in-the-tui.html`.
- **Gestures, decided:** Ctrl+click opens with the desktop (unchanged). Shift+click reveals in the file manager (was: open with the desktop). Alt+click opens the floating square; a second click on it promotes it to a split. Ctrl+Alt+click opens a side-by-side split at once. Super+Ctrl+click still reveals.
- **Alt+click collision:** today Alt+click anywhere on a line TD thinks is a command or starts with a URL copies that line (`copy_hint_at`, `pane.rs:4206`; branch at `pane.rs:6196`). Resolution proposed at Gate 2: a drawable document under the pointer opens; anything else copies as today; "Copy link" joins the right-click menu; the Alt-held chip names what the click will do.
- **Ctrl+Alt+click is free:** Hyprland binds the mouse only with Super (`hyprctl binds`, modmask 64 on mouse:272/273). In TD it currently falls into the copy branch, then the Ctrl open branch.
- **Hyperglow** = `float_shadows(th.accent)` with `.border_2()` in the same hue (`main.rs:22187`), the stack every modal and menu uses.
- **Formats now:** images, Markdown, HTML. Program-drawn pixels (Kitty/Sixel) are future work, except the size-answer fixes, which ship with the first build (see the stale-size issue, parker-brown-family/terminal-delight#718).
- **HTML:** starts as an image of the page, behind a pluggable engine interface; TD draws the note affordances and writes notes into the file in the brief's own format, so the file still works in a browser. Servo only after a fidelity test on the report archive.
- Documents bend with the CRT glass in the first build; a flat exemption only if dense text reads badly.
