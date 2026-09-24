# Status: GUI in the TUI — documents open in the pane

**Difficulty: 7/10** — it writes notes into Parker's brief files, changes four click gestures he uses every day, and the HTML engine is a fork he called "a REALLY HARD question"; a wrong call there stays expensive → **four gates**.
**Turned out to be:** _filled in at the end_

- Gate 1 — Product: **APPROVED 2026-09-24** — through Parker's notes on the research brief and his chat message the same day. `01-product.md` records those decisions; its success metric and announcement are mine and were not in his notes — strike them if wrong.
- Gate 2 — Architecture: **APPROVED 2026-09-24** — Parker: "LGTM --- LFG!" on `02-architecture.md` as drafted, before the snapshot spike reported. If the spike contradicts the notes-layer design (anchor ids that differ from a browser's, or an unsafe write), Gate 2 goes back to in progress.
- Gate 3 — Program Design: in progress
- Gate 4 — Slice plan: pending

## Slices
_Written at Gate 4. The tracer-bullet candidate: Alt+click on a PNG path opens it in a floating square with the hyperglow, Esc closes it._

## Notes for a fresh session
- Calibration: Gate 2 came back as a bare approval (nothing changed). If Gate 3 does too, the 7/10 was high: fold the slice plan into Gate 3 and start building.
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
