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

1. **Built, tested** — Shift+PgUp/PgDn, Alt+O, Alt+Q with digits, pane marks, back/forward, Alt+E, lookup age, the mark chip.
2. Scroll mode on Alt+[ (j/k, [ ], { }, e, g/G, /, q).
3. Constellation: projects, then their tabs; new project and new tab from there.

## Facts verified

- The workbench shelf on Alt+1–4 cannot fire while the digits are on: the Window rung is ahead of the Bench rung. The help sheet now says "Alt+1 … 4 (marks off)".
- Terminal Delight does not parse shell prompt marks (OSC 133): they appear only in the `vt/lab` fixtures.

## Post-hoc difficulty

Unset. It gets filled in when slice 3 lands.
