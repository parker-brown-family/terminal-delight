# Plan: an `ls` listing opens by Alt+click

**Difficulty 5/10**, scored before any code. The click table, the floating square and the pane beside all exist already. What is new is a file name with no slash in it, and two decisions sit in front of the code: what "a glow pane" is, and how a bare name finds the folder it was listed from without opening the wrong file.

## What it is for

Parker, 2026-10-15: *"We would love to get an LS from the raw terminal to be alt clickable and open a glow pane in Terminal Delight."*

Alt+click opens a path today only when the path has a slash in it: `/x`, `~/x`, `./x`, `docs/x.md`. `ls` prints bare names such as `README.md`, so a listing is dead to the pointer. On Omarchy, `ls` is `eza -lh --group-directories-first --icons=auto`, which prints those bare names at the end of each long row, after an icon.

## What the code says today

- `link_of_token` (`app/src/pane.rs`) takes a token as a path only when it starts with `/`, `~/`, `./` or `../`, or contains a `/`. `README.md` is not a link, so Alt+click, Ctrl+click and Shift+click all pass it by.
- Both terminal cores already keep OSC 8 hyperlinks on the cells they paint (`vt/rio.rs`, `vt/alacritty.rs`), but the accessor is test-only: *"TD finds links in the text itself and does not act on OSC 8"* (`vt/types.rs`).
- `eza --hyperlink` and GNU `ls --hyperlink` put the exact absolute path behind every name they print. Measured in a scratch folder: eza writes `file:///…/lsdemo/my%20notes.md` behind `'my notes.md'`; GNU ls writes `file://legion/…/README.md`, naming this machine. `reveal_target` accepts `file:///` and `file://localhost/` and refuses every other host, so GNU ls's links would be refused as written.
- `glow` is not installed here (`extra/glow 3.0.0` is in the repositories) and has never been run on this machine.
- The host only ever starts shells. An agent being resumed is typed into its new shell (`host.rs`, `spawn_pane`). A pane that runs a program is that same move: a new way TD writes into a terminal, which `docs/security/terminal-input.md` has to list, and a file name quoted for whichever shell the pane runs. A file name is attacker-controlled in any cloned repository.

## The shape (recommended: Terminal Delight's own view)

1. **Read the hyperlink first.** When the clicked cell carries an OSC 8 `file://` link on this machine, that is the first reading: the exact path, spaces and all, whichever folder was listed and wherever the shell has gone since. `Term::hyperlink` stops being test-only on both cores, and `reveal_target` learns that this machine's own hostname is local. Only `file://` links are honoured. A web address hidden behind a label is not followed, because the label can say anything.
2. **A bare name falls back to the pane's folder.** With no hyperlink, a bare name is tried against the pane's current directory at click time, and taken only if the file is there. It never colours a word: the disk is asked at the click, never while painting.
3. **The click table does not change.** Alt floats it in the square, Ctrl+Alt opens it in a pane beside, a second Alt+click on the square's own file promotes it to a pane, Shift reveals it, Ctrl hands it to the desktop. A listing behaves like every other path.
4. **Parker's alias carries the paths.** `alias ls='eza -lh --group-directories-first --icons=auto --hyperlink=auto'` in `~/.bashrc`, below the line that sources Omarchy's rc. `auto` writes links only to a terminal, so pipes, scripts and agents keep seeing plain names.

If "a glow pane" means the glow program, Alt+click on a Markdown name opens a pane beside and types `glow -p -- '<path>'` into its new shell. That needs glow installed on every machine, a new row in the terminal-input manifest, quoting that is right for bash, zsh and fish alike, and it makes Alt+click on a `.md` mean two different things depending on whether the path came from `ls`. glow reads Markdown only. The square also reads HTML, pictures, PDFs, video and sound, and takes notes.

## Least confident decisions

1. **What "a glow pane" means.** Recommended: Terminal Delight's own view, above. The alternative is the glow program.
2. **The bare-name fallback can open the wrong file.** After `ls docs/`, a click on `README.md` opens `./README.md` if one exists here as well. The same happens after a `cd`. The hyperlink makes Parker's own listings exact. The fallback is what makes stock Omarchy and agent panes work at all.
3. **No hover look for a hyperlinked name in slice 1.** eza's colours stay as they are; only the click changes.
4. **A pane on another machine.** An eza `file:///` link printed on a remote host names a path on that host, and TD cannot tell the difference. That is already true of every absolute path a remote pane prints.

## Slices

1. **Tracer bullet:** a pane fed eza's real OSC 8 bytes; a real Alt+click on `README.md` opens the square on the hyperlinked file, and on the same name in another folder it opens that folder's file.
2. GNU ls's own-hostname links; a name with a space in it; a hyperlink wrapped across two rows.
3. The bare-name fallback against the pane's folder, checked on disk.
4. The alias line in Parker's `~/.bashrc`; the gates on both cores; one pull request.
