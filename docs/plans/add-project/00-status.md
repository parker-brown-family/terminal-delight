# Add a project from the composer: a machine, then a folder, or clone or start new

**Difficulty: 6 / 10**, scored before any code: one new dialog inside the composer,
but it reaches other machines over ssh, writes two of Parker's config files
(`machines.toml`, a new `projects.toml`) and makes folders and clones on disk. A wrong
folder or a bad config line shows up the first time it is used and costs a delete to
undo. Six buys one plan page; this is it. Parker said "Get that functionality for new
project into our build of TD", so the calls nobody made are recorded in
`add-project.calls.json` for him to stamp or overturn rather than asked first.

Branch: `explore/t3-scuttle` (never a launch branch), because the composer lives only
there. Ticket: APES `add-a-project-from-the-composer-pick-a-machine-then-a-folder-or-clone-or-start-n-mw8rf5dg`.

## What Parker asked for (2026-11-08)

He pasted two posts from MiaAI_lab about their T3 Code fork
(`github.com/MiaAI-Lab/t3code`, MIT, forked from `pingdotgg/t3code`), and a screenshot
of our own composer with a GitHub URL typed into the sentence and PROJECT set to
`agent-skills`, because the repository he meant was not a project TD knew. Then:

> Get that functionality for new proejct into our build of TD

The post, verbatim:

> New in my T3 Code fork: adding a project, simplified.
> - Click the new 📁+ icon next to New thread
> - Pick a machine: this computer, or any SSH host from your config (it connects for you)
> - Click through to a folder and press Add project
> - Clone or start empty from the same dialog

Their dialog (commit `8bc3950626`, `AddProjectDialog.tsx`): *Add project — Where is the
project?* lists every machine with its connection state, then SSH hosts from
`~/.ssh/config` not added yet; picking one connects it. The next step, *Add project on
spark1 — Pick a folder. A missing folder is created.*, is a path field over a folder
list (`..` first), Back and Add project. *Clone or create new…* hands over to the
palette's existing clone and new-project flows.

## The shape, in TD's terms

TD already has the two halves this needs: the composer's PROJECT chip (a depth-one scan
of `~/Work` and four other roots) and the MACHINE chip (`machines.toml`, an agent
started as an `ssh -t` line). What it lacks is a way to name a folder the scan cannot
see, on this machine or a far one, or to make one.

- **Where it opens.** `ctrl+o` in the composer, a `＋ add` button beside the PROJECT
  chip, and a first row in the PROJECT list. The composer is TD's *New thread*; the
  inbox spine's `+ NEW` stays alone, because the spine's other face already has a
  *project* (a group of tabs) and a second kind of project button there would read as
  the same thing.
- **Step 1, where is it?** This machine; each far machine from `machines.toml`, checked
  over ssh when the dialog opens (checking → reachable / cannot reach, with ssh's
  reason); then every `Host` in `~/.ssh/config` that is not one of those yet. Git hosts
  (a `HostName` on a known forge, or `User git`) are left out, and the dialog says how
  many. Picking a new host checks it and appends it to `machines.toml`.
- **Step 2, pick a folder.** A path field (`~/Work/` to start) over the folders in it,
  filtered by what is typed after the last `/`, git checkouts marked. ↑↓ choose, ↵
  opens a folder, `ctrl+↵` adds the folder in the field, Tab completes. The line above
  the button says what Add will do, including *does not exist yet: it will be created*.
  Far folders are listed by one small POSIX script over ssh.
- **Clone or start new.** One field: a git URL (`https://`, `ssh://`, `git@host:`,
  `host:owner/repo`, or `owner/repo` meaning GitHub) clones into the folder step's
  folder under the repository's name; a plain name starts an empty git repository
  there. A URL already in the composer's sentence is offered in the field, which is
  Parker's screenshot exactly. Git's progress line shows while it runs; Esc stops it.
- **Afterwards.** The dialog closes onto the composer with PROJECT set to the new
  project and MACHINE set to its machine. A folder the scan cannot see is remembered in
  `projects.toml` beside `machines.toml`. A far project carries its machine: sending it
  on another machine is refused with a note instead of starting in a folder that is not
  there.
- **ssh manners.** Every call is `BatchMode=yes` (never prompts), with a short connect
  timeout and a private ControlMaster socket so browsing a far machine is one handshake.
  A host whose key is not trusted yet is refused with *run `ssh <host>` once in a
  terminal*; the dialog never accepts a host key for anybody.

Not built: the LAUNCH AGENT panel (Alt+A) does not list added projects; far projects
are not re-checked after they are added; nothing removes an added project except
editing `projects.toml`; no remote repository picker (`gh repo list`), which their
palette has.

## Progress

- [x] Plan, calls, ticket. Two calls stopped under the standard posture (both readings
      medium); Parker answered both with the recommendation, then said he was away and
      not to ask again, so every later call is recorded and carried.
- [x] `addproject.rs`: ssh hosts (with `Include`, patterns and git remotes), clone
      targets, the path field, listings, `projects.toml`, the machine append, the far
      scripts — pure, 18 tests, the far scripts run for real through `sh -c`
- [x] `addproject_view.rs`: the dialog's state, keys, background reads and runs, drawing
- [x] Composer: Ctrl+O, the `＋ add` beside PROJECT, the list's first row, far projects
      carry their machine, the send guard, `projects.toml` read on open
- [x] Strings in nine languages (56)
- [x] Window tests with a stand-in `ssh` that runs the far script on this box: six, end
      to end (pick, make, far, new host, clone and start, Esc and both doors)
- [x] Photos on the private display (a stand-in home and a stand-in `ssh`; one real
      clone of the public repository into the stand-in home), reshot after the layout fix
- [x] Six mutations, each caught; real ssh to farbox tried once by hand (read-only
      listing: 0.30 s, then 0.06 s over the shared connection, nothing hung)
- [x] One Codex review round (read-only, said out loud): seven findings, all fixed
- [x] Brief (`reports/2026-11-08-add-a-project.html`), pushed, the test window on workspace 7
      reopened on this build (its terminal host and the t3 shells kept)

## Surprises

- The first photographs showed every list in the dialog squeezed, the last row cut in
  half. The card was `w_full().max_w(640)`, and its height was worked out before its
  lines wrapped at 640, so it came out a line short and the lists, which may shrink,
  gave the line back. A width of its own fixed it; the composer's own card has the
  opposite symptom (empty space at its foot) for what looks like the same reason, and
  was left alone.
- gpui's test keystrokes carry no typed character until `with_simulated_ime` gives them
  one, which is why the window tests can type paths and URLs at all.

## Review round one (Codex, read-only)

Seven findings, each checked against the code and fixed, with a test where a test can
reach it. It found no shell-word injection in the far scripts or their quoting.

1. A host whose ssh config says `StrictHostKeyChecking accept-new` would have had an
   unseen key taken: `BatchMode` stops the asking, not the accepting. Every call now
   carries `StrictHostKeyChecking=yes`.
2. An `Include` inside a `Host` block lost its keys, so `User git` kept in an included
   file let a git remote through as a machine. The included file now starts inside the
   block it stood in.
3. A far folder whose name holds a line break printed as two folders, and a row could
   open one that is not there. Such names are left out on both sides.
4. A plain Enter trusted a listing read earlier: a folder deleted since would have been
   made again. The add now looks at the moment of adding, and only Ctrl+Enter or the
   button makes anything.
5. Two windows adding at once could lose a line or collide on one temp file. Writers
   take turns on a lock file, each with its own temp file.
6. A far project's tab could be filed beside a local terminal standing at the same
   path. A far project is filed by name only.
7. A long ssh config ran off the card with the highlight out of sight. The machine
   list keeps a window of eight, follows the highlight, and counts what is below.

My own reading before the round added two: a run dropped with the composer now kills
its program, and a run stopped with Esc never becomes the project.

## Outcome

**Post-hoc: 6 / 10; the score held.** One plan page was the right size: the two asks it
raised came back as the recommendation both times, and everything after was carried
under "do not ask any questions". What the score did not predict was where the work
would be wrong: not in the dialog's logic, which held through six mutations, but in the
corners a second reader found (a config's `accept-new`, an `Include` inside a block, a
name with a line break, a listing gone stale, two writers at once) and in a layout that
only a photograph showed. The suite was run three times on a machine at a load average
of 40 to 70 from other agents; each run failed one or two different timing tests in
areas this work does not touch. Three of the four passed every time alone; the fourth,
which reads a printed link off a terminal screen, failed once in three alone too and was
filed as issue 1398 (the handover scan reads before the wrapped half arrives).

