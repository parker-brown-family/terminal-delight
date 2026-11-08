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
- [ ] Model: `flyout.rs` (pure: tabs, targets, diff and numstat parsing, `gh` JSON, syntax).
- [ ] Runner: `engstate::run_bounded` generalised from `run_git_bytes` (stderr kept for `gh`).
- [ ] View: `flyout_view.rs` (chip, panel, tab strip, new-tab page, three tab kinds).
- [ ] Wiring: stage row, spine chip, chord, keys, focus, poll loop, persistence.
- [ ] Ways in: Alt+click and menu row (`docopen`), rail handovers (`pane/handed.rs`).
- [ ] Strings in nine languages (`lang/flyout.rs`) and the leftover-English guard.
- [ ] Tests: parsers, layout (panes shrink, nothing covered), keyboard (no leak),
      persistence round trip, link routing.
- [ ] Seen in a real hidden window, photographed; full suite, clippy, fmt.
- [ ] Brief for Parker, ws7 window on the build.

## Surprises and discoveries

- `gh` in the window's PATH is mise's shim, not `~/bin/gh` (the account wrapper), because
  `~/.local/share/mise/shims` comes first. It answers as `parker-brown-family` today. For
  a read-only view that is acceptable; `TD_GH` names another binary.
- Alt+click on a code file or a PR URL did nothing before this (the click table answers
  `Pass`), so the flyout takes a free gesture rather than changing a known one.

## Outcome

(written when the work lands)
