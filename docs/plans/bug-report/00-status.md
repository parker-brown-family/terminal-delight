# 🐞 Report a bug: status

**Difficulty submitted before work: 5/10.** That buys one plan page and one approval (this page). The button is small. The score comes from what sits under it: missing logs fail silently, and nobody notices until the day they are needed.

**Approved:** Parker, 2026-10-19: *"build it!"*, after *"we CANNNNNOTTT!!! take folks actual terminal session input or content! and if we are asking them to make submission fo bug data - must follow the consent ladder, yes"*.

## What was wrong

Measured on 2026-10-19 on Parker's running window, not inferred from code:

- The running window and its session host both had stderr on `/dev/null`. TD has no logging library; it has 168 `eprintln!` calls, and every one of them went nowhere.
- The host was meant to log beside its socket, in `$XDG_RUNTIME_DIR`, a directory a reboot empties. The live host had no log file at all.
- `~/.local/state/terminal-delight/window.log` was last written on 12 September.
- There was no panic hook. A Rust panic exits without a core dump, so Omarchy's crash watcher, which reads core dumps, never sees TD crash. A panic on a background thread ends that thread and leaves the window running, so a pane can freeze with nothing said anywhere.

## The rule

A report never carries a person's terminal input or output: no scrollback, rows, pane titles, working directories, command lines or screenshots. It carries TD's builds, the system's versions, counts, crash locations in TD's code, and TD's own diagnostic lines with every path, link and quoted value scrubbed out.

## The consent ladder

"Consent ladder" was not written down anywhere on disk before this. Here it is defined as: each rung moves the data one step further from the person, each rung is its own click, and nothing climbs a rung by itself.

| Rung | What happens | Where the data is |
|---|---|---|
| 0 | Logs and crash records are written | this machine |
| 1 | 🐞 → *Prepare the report* writes `report.md` and opens it in the document square | this machine, shown to the person |
| 2 | *Draft the issue with claude/codex* opens a tab in the report's directory, told to read that file | the agent's provider |
| 3 | The agent shows the exact issue and files only on a yes; or *File it yourself* opens GitHub's new-issue page | GitHub |

## What was built

- `app/src/diag.rs` is new. It holds:
  - `route_stderr`: sends stderr to `~/.local/state/terminal-delight/logs/{window,host-<key>}.log`, but only when nobody is reading it (not a terminal, not already a file). A log over 4 MiB is set aside as `.log.1`.
  - `install_crash_record`: a panic on any thread writes `crashes/<ns>-<pid>.txt` with build, role, thread, code location, a scrubbed message and the backtrace.
  - `scrub`, the report itself, and the agent hand-off.
- `main.rs`: the window routes and records at startup. It also gets the 🐞 glyph between 🔔 and 🧩, with an orange dot when a crash was recorded since the last report; the panel; Escape handling; and a 🐞 entry in the narrow window's … menu.
- `host.rs`: the host routes and records at startup.
- `hostctl.rs`: a host the window starts logs to the persistent logs directory, opened once.
- Strings in all nine languages.

## What the tests prove, and what they do not

- `diag::tests` covers:
  - paths, links and quoted values are scrubbed, including a path containing a space;
  - TD's own wording and numbers survive;
  - Rust's string-slice panic message loses the string it quoted;
  - a report keeps only TD's operational lines (developer traces like `[doc-hit]`, which quote the token and cwd under the pointer, and the agent launch line never qualify);
  - the census counts kinds and never names a program;
  - the agent's run line is one single-quoted argument;
  - a whole report, built from fixture logs and a crash record that hold a password, a client's directory and a program's name, contains none of them, and does contain the agent's rules.
- **A real panic through the installed hook** — `a_real_panic_on_a_thread_leaves_a_scrubbed_crash_record` runs a child copy of the test binary with its state directory in a temporary folder, panics twice on a named thread (a bad string slice and a formatted message quoting a token), and checks both records and stderr.
- **Mutations, 2026-10-20:** with the scrub taken out of the report's line reader, two tests fail, including the whole-report test; with the hook keeping the raw panic message, the real-panic test fails on the quoted token. The first version of that test did NOT fail under the mutation: Rust 1.98 quotes only the character a bad slice landed inside, not the whole string, so the drill carried nothing secret. The drill was rewritten until the mutation failed it.
- **Live check, 2026-10-19:** a real `serve` started with stderr on `/dev/null` showed stderr pointing at `logs/host-<key>.log`, and its startup error landed there.
- **Not exercised by a test:** the panel drawn on screen, and the agent tab opening. Those are tested by hand in the installed build.

## Least-confident decisions

1. Developer debug traces still write terminal text (token, cwd, row heads) to the local log when someone sets `TD_HITDEBUG` / `TD_ANCHORDEBUG` / `TD_SELDEBUG`. They never reach a report, but they do reach the disk.
2. A path printed without quotes is cut to the next clause break. That can remove some of TD's own wording after it, the safe way to be wrong.
3. Log lines carry no timestamps; only each process's header line does. Adding them would mean a pipe and a thread, and a crash would lose the last lines in that thread's buffer.
