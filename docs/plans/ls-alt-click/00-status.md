# Status: an `ls` listing opens by Alt+click

**Difficulty: 5/10** — the click table, the square and the pane beside exist; a bare name and what "a glow pane" means are the decisions → one gate (`plan.md`)
**Turned out to be:** 4/10. Parker took all three recommended answers unchanged, and the work was one reading put in front of an existing click path, about a hundred lines with its tests. The decision that needed him was the glow question; the rest could have been built straight from the request.

- One-gate plan: APPROVED 2026-10-15, as three answers: "TD's own view" (not the glow program), "hyperlink, else here" (plus `--hyperlink=auto` on his alias), "build all four"

## Slices
- [x] Slice 1 — tracer bullet: eza's OSC 8 link under a real Alt+click opens the hyperlinked file
- [x] Slice 2 — GNU ls's own-hostname links, names with spaces, a link wrapped across rows
- [x] Slice 3 — bare-name fallback against the pane's folder
- [x] Slice 4 — Parker's `ls` and `lt` aliases gain `--hyperlink=auto` (`~/.bashrc`, backup beside it); gates on both cores; one PR

## Notes for a fresh session
- Worktree: `.worktrees/ls-alt-click`, branch `ls-alt-click`, cut from `origin/main` at 995ec1d.
- `glow` is not installed on this machine (2026-10-15), and Parker chose TD's own view, so nothing here starts a program.
- eza's `--hyperlink=auto` writes links only to a terminal that has a size. A pseudo-terminal from `script` with no size gets none, which looks like the flag failing; `stty cols 100 rows 30` first and it writes them. A real pane always has a size.
- GNU `ls --hyperlink` names the machine (`file://legion/…`) and canonicalizes the path; eza writes `file:///…` with the path as given.
- Known gap, by design: with no hyperlink, `ls docs/` then Alt+click on `README.md` opens `./README.md` if the current folder has one.
