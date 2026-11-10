# Settings: one book for every setting, laid out the way T3 Code lays out its own

**Difficulty: 8 / 10**, scored before any code. The settings live in about fifteen floating
panels across the window's biggest file (`main.rs`, 53,000 lines). Each panel carries its own
keyboard and its own Escape order. Every skin has to draw the new surface. And a setting
dropped on the way across stays missing until somebody goes looking for it. Eight would buy
four gates. Parker is away ("just keep on building us into the future") and has already
named the product (Gate 1) and the design standard (Gate 2's look), so the rest collapses
into this page. The calls nobody made are recorded in `settings.calls.json` for him to stamp.

Branch: `explore/t3-scuttle` (never a launch branch).

## What Parker asked for (2026-11-09, evening)

> look at the general setting management and every app that has settings... we should have
> known we would end up here... so SETTINGS being the sort of floating mess that it is right
> now will get competely overhauldd to look modern and proper ... CONFIG VIEW --- OPENS LIKE
> A BOOK OVERLAY... not unlike agents ... the surface config is beautiful... but the SETTINGS
> will be an ENTIRE MONOLITH like any moder app using T3 as a design standard now. How we
> organize each page can be creative, but the section heading and names and glyphs will
> reflect what T3 code does...

His three screenshots: T3 Code's Settings › Worktrees page; TD's Agents panel (the "book":
one card, two pages, a hairline between them); and TD's surface config (THEME · STYLE ·
RULES · UI CONFIG), which he called beautiful.

## T3's standard, read from its source (pingdotgg/t3code, MIT)

`apps/web/src/components/settings/settingsSearch.ts` (`SETTINGS_NAV_GROUPS`,
`SETTINGS_SECTION_LABELS`) and `SettingsSidebarNav.tsx` (lucide icons):

| Group | Pages (icon) |
|---|---|
| — | General (Settings2) · Appearance (Palette) · Chat (MessageSquareText) · Diffs & pull requests (FileDiff) · Keybindings (Keyboard) · Voice (Mic) · SnapShots |
| Agents | Providers (Bot) · Threads (MessagesSquare) · Browser (Globe) · Devices (Smartphone) · Scheduled tasks (CalendarClock) |
| Code | Projects (PanelsTopLeft) · Source control (GitBranch) · Worktrees (FolderTree) |
| Connections | Environments (Server) · Sharing & access (ShieldCheck) · Secrets (KeyRound) |
| — | Maintenance (Wrench) · Labs (FlaskConical) · About (Info) · Archive (Archive) |

A page is a breadcrumb (`Settings / Worktrees`) and a close ✕ over sections. Each section is a
title in sentence case ("Cleanup") over a rounded group of rows. A row is a title, one plain
line under it, and the control on the right, with a reset mark when it differs from the
default and an "Advanced" fold for the rarely used. A search box sits above the nav and
ranks results title-exact, then title-prefix, then title-contains, then every word, then
anywhere (`searchSettings`).

## The shape in Terminal Delight

- **A book over the window**, the Agents panel's frame grown to the window: the left page is
  the nav, the right page is the setting. It opens with **Ctrl+,** (T3's chord and
  everyone else's), from a **⚙** at the head of the menu-glyph corner that cannot be hidden
  (like CONFIG, it is the way back), and from the CONFIG dropdown.
- **T3's names, groups and glyphs** for every page TD has something to put on. Pages TD has
  nothing for (Chat, Diffs & pull requests, Voice, SnapShots, Browser, Devices, Scheduled
  tasks, Worktrees, Secrets) are left out, not drawn empty. Glyphs are the nearest single
  characters to T3's lucide icons, and a text-controls skin spells them as words.
- **The surface config stays as it is.** Parker called it beautiful; Appearance opens it, and
  hosts the window-wide switches that are not the look itself.
- **Rows in the skin's own ink.** T3's anatomy (title, plain line, control right, groups
  with hairlines), drawn with the skin's radius, border and colours, so hacker still looks
  like hacker.

| TD setting, where it lives today | Settings page |
|---|---|
| Language (lang picker) | General |
| Window scale (scale menu) | General |
| Updates (update panel) | General |
| Look palette, gauges (🎨, 📊) | Appearance (opens them) |
| Agent colours (terminal state follows outer) | Appearance |
| Agent-finished notices (🔔) | Notifications |
| Keys sheet (F1) | Keybindings |
| Launch defaults, local agent command, TD Skills (🤖) | Providers |
| Rest and sleep (🤖) | Threads |
| Projects (`projects.toml`, add a project) | Projects |
| Far machines (`machines.toml`) | Environments |
| Threshing Floor and MCP switches | Sharing & access |
| Memory savings, bug report, admin | Maintenance |
| Plugins (🧩) | Labs |
| Build: version, commit, wire, core | About |
| Closed panes (graveyard) | Archive |

## Slices

- [x] 1 — tracer: the book, the nav, search, Ctrl+, and ⚙; native pages for Threads,
  Sharing & access, Providers, Appearance's switch, General's language, About; every other
  page carries a row that hands off to the panel it still lives in, so nothing is lost.
- [ ] 2 — move notifications, keybindings, maintenance, plugins and the archive onto native
  pages; the old glyph menus open Settings at their page.
- [ ] 4 — **Code › Worktrees, with T3's Cleanup** (Parker, 2026-11-09, mid-build: *"oh and
  the worktree cleanup -- yeah we should go for that 100% <3"*). T3's rules: delete worktrees
  whose threads were deleted, whose pull request is merged and commits are in the default
  branch, that sat inactive N days, that settled N days ago, that have no commits past the
  default branch; keep worktrees with local changes. In TD a "thread" is an agent pane, so a
  worktree is in use while any pane's cwd sits in it. Deleting is the one-way door here, so
  the page first lists what each rule would remove and why, removal needs a second press,
  and a worktree with uncommitted changes is never a candidate, whatever the switches say.
- [ ] 3 — UI CONFIG's visibility switches as Appearance › Interface rows; a sweep of every
  skin and design; translations checked.

## Progress

- 2026-11-09 slice 1 built: `settings.rs` (pages, T3's groups and order, glyphs, T3's search
  ranking), `settings_view.rs` (the book), `lang/settings.rs` (nine languages), Ctrl+, from a
  pane (`pane::ToggleSettings`, `keylayer`), ⚙ leading the corner, the TD_SKIN_SCENE=settings
  fixture with TD_SETTINGS_PAGE / TD_SETTINGS_QUERY. Rig `~/Work/td-t3-scuttle/rig/settings-shoot.sh`.
  Suite 3,542 / 0, clippy clean.

## Surprises and discoveries

- **Letters typed into the search also reached the shell.** `overlay_owns_keyboard` only stops
  the render handing focus back to a pane; it never takes focus. A book opened with Ctrl+,
  from a pane leaves that pane focused, so the pane wrote every letter to its shell while the
  workspace searched with the same letter. The screenshots could not show it; the window test
  did, once it was made to watch the shell. The book now takes its keys in the capture phase,
  as the help sheet does.
- **The book bowed with the CRT tube** until it joined `warp::set_suppressed`. The guard test
  `every_overlay_esc_can_close_also_flattens_the_glass` would have failed on it, and then
  misread `settings_last` inside `close_popups` as a panel; that logic moved to `settings_peel`.

## Outcome
