# The right-bar flyout: diffs, code files and pull requests beside the conversation

**Difficulty: 7 / 10**, scored before any code: a new docked region in the window's
biggest file, three kinds of content backed by outside processes (`git`, `gh`), a
keyboard that must never leak typing into a terminal, and state that has to survive a
restart. Seven buys four gates. Parker asked for this at night, said "use your own
judgement and build end to end on that experimental branch", and went to sleep, so the
gates collapse into this one page. Every decision nobody made is recorded as a call in
`flyout.calls.json`, for him to stamp or overturn.

Branch: `explore/t3-scuttle` (never a launch branch). Ticket: APES
`right-bar-flyout-beside-the-attention-rail-tabs-for-a-repo-s-diff-code-files-and-mw7nbvei`.

## What Parker asked for (2026-11-08, night)

> we are thinking about Right Bar flyout ... to ADD another chip here beside the
> attention spine for agents... specifically for code files diffs and github prs ...
> opening TABS each one with its own identity ... use your own judgement and build end
> to end on that experimental branch ... Make it beautiful and a DELIGHT to work with

His screenshots were T3 Code's side panel: a tab strip (`Diff`, a PR tab, `+`), a
"Changes vs origin/main" file list with per-file `+/-` counts that expands, a `+` menu
(Browser, Terminal, Files, Diff, Pull request, ...), and a PR page (title, state,
branches, Summary / Timeline / Code, reviewers, labels, description, checks, comments).
He also pasted an outside assessment recommending a live Chromium view over CDP for web
pages; see the call on native rendering for why this build does not start there.

## The shape

- **A chip in the spine column**, under the attention pill at the top right. It shows
  how many flyout tabs the shown task has and lights while the flyout is open. Click it,
  or press **Ctrl+Shift+R** (R for the right bar), to open and close.
- **A docked panel between the screen and the spine.** A flex sibling like the left bar
  and the docked queue: it costs the terminals its width and covers nothing, so the
  conversation stays usable beside the evidence. Its left edge drags to resize. It wears
  the docked queue's frame (two-pixel accent edge, opaque darkened fill, floating
  shadows, the skin's finish), the house recipe for a flat panel beside bent glass.
- **Tabs, each with its own identity**: a kind glyph (`±` changes, `▤` file, `⇄` pull
  request), a short name (the repo, the file name, `#123`), and the project's colour from
  the left bar. The tabs belong to the left-bar task they were opened in, so the flyout
  follows the conversation; whether it is open, and its width, are the window's.
- **`+` and the empty state are one page**: this task's repo changes in one press, this
  branch's pull request and the repo's other open ones, and a file filter.
- **Changes tab**: `git` against a base (the working tree against `HEAD`, or against the
  branch's upstream), every changed file with its status and `+/-`, untracked files
  counted as added. A file expands to its hunks with old and new line numbers, added and
  removed lines tinted in the skin's `ok` and `danger` inks (never the accent, which is
  the furniture colour). It re-reads every three seconds while it is on screen, so an
  agent's edits appear as they land.
- **File tab**: the file with line numbers and light syntax colour from the terminal's
  own ANSI palette, lines changed against `HEAD` marked in the gutter, virtualised with
  gpui's `uniform_list` so a 50,000-line file scrolls like a short one. Reloads when the
  file changes on disk.
- **Pull request tab**: from `gh pr view --json` and `gh pr diff`. Header (repo and
  number, state, title, author, age, base ← head, files and `+/-`), then three faces:
  Overview (description through TD's own Markdown renderer, checks, reviewers, labels),
  Files (the same diff view as the Changes tab), Conversation (comments and reviews).
  Opening it on GitHub is one press. Read-only: no merge or checkout buttons.
- **Ways in**: the chip and chord; the `+` page; **Alt+click** on a GitHub pull-request
  URL or on a code file in any terminal (today that click does nothing on those); a
  right-click menu row; and a pull request an agent hands over on the attention rail,
  which opens here instead of asking about the browser.
- **Keyboard while the flyout holds it**: arrows and PgUp/PgDn scroll, `j`/`k` walk
  files, Enter or Space expand, `[`/`]` or `1`–`9` switch tabs, `w` closes a tab, `r`
  re-reads, `o` opens on GitHub, Esc gives the keyboard back to the pane. Nothing typed
  there reaches a terminal.

## Deliberately not built

- **A live web page (CDP input, frame streaming).** Parker asked for code, diffs and PRs;
  native rendering is faster, wears the theme, and needs no GitHub cookies in a browser
  profile. The tab model has room for a page kind; the page engine's own comments
  already anticipate a live engine.
- **Merge, checkout, comment.** One-click writes to GitHub are outward-facing; an
  agent-built panel should not add them without Parker deciding.
- **An MCP verb for agents to open things here.** The rail's handover path covers the
  agent's pull request today without new MCP surface.

## Progress

- [x] Orientation: APES orient and context, Fieldcraft, the five subsystems mapped by hand
      (the fan-out guard refused a reader workflow, as Parker's rule says it should).
- [x] Model: `flyout.rs` (pure: tabs, targets, numstat and name-status, unified diff, the gutter,
      GitHub addresses and remotes, `gh` JSON, timestamps, a syntax colourer, the file finder).
- [x] Runner: `engstate::run_bounded`, generalised from `run_git_bytes` (git unchanged; stderr kept
      for `gh`).
- [x] View: `flyout_view.rs` (chip, panel, tab strip, `+` page, Changes, File and pull request tabs).
- [x] Wiring: stage row, spine chip, Ctrl+Shift+R, keys, `overlay_owns_keyboard`, resize, 1 s poll,
      persistence (`Tab.side` through `TabIdentity`, `SavedTab.side`, `StateFile.flyout`).
- [x] Ways in: Alt+click and the right-click row (`docopen`), rail handovers (`pane/handed.rs`).
- [x] Strings in nine languages (`lang/flyout.rs`), registered with the leftover-English guard.
- [x] Tests: parsers, git against scratch repositories, `side_of` on real files, a window test
      (panes narrow, nothing covered, keys stay in the flyout, Esc gives them back), persistence,
      the click table, the handover path.
- [x] Seen in a real window: photographed and driven with xdotool on a private Xvfb display,
      including a real Alt+click on a printed `path:line`.
- [x] One review round from another provider (Codex, read-only): five findings, all real, all
      fixed, the two with tests checked by putting the old code back.
- [x] Brief for Parker (`~/Work/terminal-delight/reports/2026-11-08-right-bar-flyout.html`); the
      workspace 7 t3 window relaunched on the build, attached to the same t3 shells.

## Surprises and discoveries

- `gh` in the window's PATH is mise's shim, not `~/bin/gh` (the account wrapper), because
  `~/.local/share/mise/shims` comes first. It answers as `parker-brown-family` today. For
  a read-only view that is acceptable; `TD_GH` names another binary, and `gh` now runs from a
  checkout of the pull request's repository when one is open, so a wrapper that picks the
  account by the remote would pick right.
- Alt+click on a code file or a PR URL did nothing before this (the click table answered
  `Pass`), so the flyout takes a free gesture rather than changing a known one.
- **Parker's git config prints `c/` and `w/` in diff headers** (`diff.mnemonicPrefix`). A test
  against a real repository read every changed path as `w/src/a.rs`. Every `git diff` the flyout
  runs now passes `--src-prefix=a/ --dst-prefix=b/`.
- **The render's focus plan handed the keyboard straight back to the pane** one frame after the
  flyout took it, because nothing told it the flyout held the keyboard. The window test caught it;
  `overlay_owns_keyboard` now counts `flyout.focused`.
- **The monitors were off overnight**, so a hidden window on the live compositor could not be
  photographed. The pictures came from a private Xvfb display with software Vulkan (lavapipe),
  which also let the Alt+click gesture be driven for real.
- **The Alt+O test used a pull request as its example of a web artifact.** A pull request handed
  over now opens in the flyout and never leaves TD, so the ask-before-the-browser rule is tested
  on a web page, and a new test pins the pull request path.
- **The first width test passed on the old code.** At the 800 px case it chose, the old rule
  happened to leave the panes about half; the test was rewritten around a 640 px tile, where the
  old rule left the panes 138 px, and it now fails on the old rule.

## Review round (Codex, read-only, one round)

1. A narrow window let the panel's own minimum push the panes under their reserve. Fixed:
   `panel_width` keeps the reserve when the window has it and splits evenly when it does not.
2. A list of only binary files showed `+0 −0`. Fixed: `flyout::totals` keeps "counted nothing"
   apart from zero and counts the files it could not count.
3. With the `+` page up, the poll also read the hidden tab behind it. Fixed.
4. A pull request opened from a link ran `gh` from the home folder. Fixed: from a checkout of that
   repository when one of the window's panes stands in one.
5. A quoted path holding `\" ` broke the diff header reader. Fixed, with a test.

Also from my own reading: a line longer than 1000 bytes is cut for drawing (a minified file is
one line of a megabyte), "show more" grows by 2000 rows a press instead of all at once, a list
draws at most 1500 files and says how many more, and readings for closed tabs are dropped.

## Outcome

Built, tested and photographed on `explore/t3-scuttle`. Not built, on purpose: a live web page
(CDP), merge/checkout/comment buttons, and an MCP verb for agents. Known gaps: a diff list of more
than 1500 files is cut rather than virtualised; untracked folders are one row each, not expandable;
a `gh` that is signed out was not tried for real.

Post-hoc difficulty: **7 / 10** held. The work could have been wrong for a while unnoticed in
exactly the places the tests and the review found it (a git config, a focus plan, a narrow tile).

---

# Round 2: what Parker found trying it, and a second pass

**Difficulty: 6 / 10**, scored before any code: four asks against code already standing, each
visible the moment it is wrong, but one of them hosts the floating square's document view in a
new place, and that view is strict about where it is drawn. Six buys one plan page and one
approval; Parker's "do another pass and make it better" is the approval, and the two calls the
policy stopped on were asked and answered (both as recommended). Calls: `round2.calls.json`.

## What he said (2026-11-08, afternoon, testing on workspace 7)

> files need search --- fuzzy style --- PR [#844: "could not find pull request diff: HTTP 406:
> Sorry, the diff exceeded the maximum number of lines (20000)"] not sure if that can be
> solved... then also the draft [its tab] the tab gets an x to close on the left hand of the tab

> [tv.html in a File tab, shown as source] html does not render

> [#752's Files face] looks good for other stuff

> Great - I mean - do another pass and ... make it better? spend LOTS of tokens doing really
> really good work!

## The plan

- **Big pull requests.** GitHub refuses one whole diff past 20,000 lines (HTTP 406), but its
  per-file list (`pulls/{n}/files`, up to 3,000 files, 100 a page) carries each file's own patch.
  When `gh pr diff` is refused for size, the Files face reads that list instead. A file GitHub
  sends no patch for (too large, or binary) is listed with its counts and says so, with a way to
  GitHub; more than 3,000 files says how many were not listed.
- **Search, fuzzy.** One matcher in `flyout.rs`, fzf-style: every letter of each word in order,
  anywhere in the path; word starts, runs and the file's own name score high, gaps cost; a capital
  makes a word case-sensitive. Used by the `+` page's finder, and by a filter over the Changes list
  and a pull request's Files list (`/`, Ctrl+F or a click; best match first, matched letters lit,
  Enter opens the best, Esc clears). Inside a File tab, `/` finds text literally, every hit lit,
  Enter and Shift+Enter step, a count beside it.
- **Tabs.** The × sits at the left of every tab, quiet until the pointer is on it. A pull
  request's ⇄ takes its state's colour (open, draft, merged, closed), as GitHub colours its icon.
- **Documents drawn.** A File tab on an HTML page, a Markdown file, a picture or a PDF hosts the
  floating square's own `DocumentView` (the same engine, headless Chromium for pages), with `s`
  flipping to the source. Links in a page open beside it: a file in a new tab, a pull request as
  a pull request, anything else in the browser. Without Chromium, the source shows with the reason.
- **Reading diffs.** The file a diff list is scrolled into keeps its row pinned at the top. Words
  that changed inside a changed line are lit. A line number in the Changes tab opens the file at
  that line. A pull request's Files and Conversation faces wear a one-line header.
- **Reading files.** A File tab scrolls sideways for long lines instead of cutting them at the edge.

## Progress (round 2)

- [ ] Model: matcher, per-file pull request read, changed words, find, run overlay (+ tests)
- [ ] View: tabs, filters and finder, find bar, big pull requests, documents, sticky rows, words
- [ ] Strings in nine languages
- [ ] Window tests, full suite, clippy, rustfmt
- [ ] Photographed on the Xvfb rig (#844 for real)
- [ ] One Codex review round, said out loud
- [ ] Follow-up brief; workspace 7 window on the build
