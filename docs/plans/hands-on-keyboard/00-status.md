# Hands on Keyboard — status

**Difficulty: 6/10.** Keyboard routing runs through one precedence ladder (`LADDER` in `app/src/keylayer.rs`) whose history is chords going silently dead on one face, and pane marks must outlive a restart. That bought one combined plan page and one approval: `brief.html`, with Parker's notes saved inside it.

## Approved 2026-11-02 (Parker's notes on the plan page)

- **Alt+O** opens the focused pane's newest handover. Pressed while any document is open, it raises the ARTIFACTS rail with cards numbered 1–9, newest = 1. *"alt+o opens current -> alt + o while ANY artifact is open opens the ARTIFACTS spine"*. Alt+B stays readline's back-one-word.
- **Alt+Q** opens NEEDS ME; a digit goes to that row in one press. *"person hits alt+q to open the spine who needs me and then 3 for the 3rd from top"*.
- **Alt+1–9 are pane marks**: tap = go, hold 300 ms = mark. *"300 ms will start as baseline. this is not a common action"*.
- **Alt+E** = previous error line, Alt+Shift+E = next. Alt+A stays launch-agent.
- **Back / forward through recent panes** (up to 9) on the keyboard and the mouse's back/forward buttons, never drawn. The visible afterglow trail was a *"hard no"*; *"(DO NOT SHOW NUMBERS) this will be INTRINSIC and hidden from view"*.
- **Lookup (Alt+L) stays by name** and gains an age: *"It is very important to include the age of the pane's last visit, or action"*.
- **Constellation**: liked, but *"30+ tabs will be OVERWHELMING! maybe ONLY projects, click then tabs"*, with *"New project, new tab"* available there.
- Order: *"ALL IN ALL FAST -- we just pile all the work to get done -- VERY good tests and automated verification"*.

## Calls the agent made (nobody decided; recorded here to stamp or overturn)

- **Forward is Alt+Shift+0** (the desktop delivers it as `)`), back is Alt+0. Parker named mouse back/forward but no forward key.
- **In NEEDS ME and the artifact rail, a digit is taken with Alt still held.** Alt+Q then 3 is naturally typed without letting go of Alt; without this it would jump to pane mark 3. A digit past the last row is swallowed for the same reason.
- **A digit in NEEDS ME now goes to the row** rather than moving the cursor there, which is a change for anyone who used digits to move the cursor.
- **The tab menu no longer shows "Alt+N jumps here by position"**, because the digits mean marks now. The on/off switch stays and reads "Alt+1-9 go to marked panes (hold to mark)".
- **A marked pane shows its digit as a small chip in its header.** The plan drew it; the notes did not object.
- **Age in lookup** = the newer of your last visit (this session) and the agent's last state change; a dash when neither is known, never a zero.
- **Alt+E only knows errors by shape** (rustc, cargo test, Python tracebacks, gcc, `fatal:`, `command not found`, `…Error:`), tested against real specimens and against innocent lines such as "0 errors" and "error handling". A run of error lines counts as one.

## Slices

1. **Landed** 2026-11-03, PR 1285 — Shift+PgUp/PgDn, Alt+O, Alt+Q with digits, pane marks with their header chip, Alt+0 back/forward and the mouse buttons, Alt+E, lookup age. Ten mutation checks, all caught.
2. **Landed** 2026-11-03, PR 1287 — scroll mode on Alt+[ (j/k, u/d, g/G, [ ], { }, e/E, /, q). Its write to full-screen programs is row 11 of `docs/security/terminal-input.md`. Five mutation checks, all caught.
3. **Not built** — the Constellation: projects first, then their tabs, with New project and New tab. Tracked with the shape Parker gave in the follow-up issue *Hands on Keyboard slice 3: the Constellation* (terminal-delight#1289).

Nothing in slices 1 and 2 has been pressed on the live desktop yet; the wave-follow install is where that first happens. Not covered by a test: the panels' digits going to their row, a digit taken with Alt still held, the header chip and scroll strip as drawn, the mouse buttons, and scroll mode over Claude Code's alternate screen.

## Facts verified

- The workbench shelf on Alt+1–4 cannot fire while the digits are on: the Window rung is ahead of the Bench rung. The help sheet now says "Alt+1 … 4 (marks off)".
- Terminal Delight does not parse shell prompt marks (OSC 133): they appear only in the `vt/lab` fixtures.

## Post-hoc difficulty

**5/10 for slices 1–2**, against 6 predicted. The routing ladder made every new chord one table row plus a test. What cost time were the things around it: nine languages per string, a key-up path gpui's test harness does not send, and the terminal-input manifest. The prediction stands for slice 3, which is a new overlay surface.
