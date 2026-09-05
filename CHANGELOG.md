# Changelog

All notable changes to terminal-delight are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project aims
to follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html) once it
reaches 1.0. Until then, `0.x` minor bumps may include breaking changes.

## [Unreleased]

### Added

- **Agents can leave notes on the fridge door.** A new `leave_note` MCP tool
  posts a sticky note onto a pane's glass — a bold headline plus at most ten
  words of body ("GET MILK!" / "home at 7pm") — so a returning human reads the
  wall the way they read a fridge, before opening any transcript. Post again to
  change the note (same paper, same lean, new words), `clear` peels it,
  `pin: true` pushes the pin through so the tab flags it from the mother bar.
  The note lands on exactly the paper a human `alt+s` writes on: peel, edit,
  pin and restart-survival all behave identically, and `list_panes` now reads
  the door back — each pane's posted note rides its listing line. Writes stay
  behind the same `TD_MCP_WRITE` opt-in as every other mutation, and the
  ten-word limit refuses verbosity by name, with the count, so an agent's retry
  is an edit rather than a guess.

- **The robots came to the web wall.** A working card on `agents.html` now shows
  the playhouse robot animated — holding the tool that agent is actually
  holding, wearing that tool's face, with the verb lettered on the glass: *at
  the console*, never `Bash`. Stop working and the card goes back to its project
  art, which is the app's own precedence: a busy pane wears what it is doing, a
  resting one wears where it is.
  - The art is **published, not copied by hand**:
    `scripts/publish-robot-faces.mjs` moves the 19 scenes, their still props and
    the tool table out of `app/assets/` into `assets/robots/`, and `--check`
    fails CI on drift in either direction, orphans included. One table, two
    walls, and no second place to edit.
  - The verb is used **both** on the glass and as the live-action word in the
    recap, so the picture and the sentence are the same string and cannot end up
    describing different activities. A check asserts it.
  - Codex panes **do** wear a face, unlike the vitals bars they still cannot
    have — `tail_tool_events` reads the Codex `function_call` shape as well as
    Claude's `tool_use`, so their cards hold `exec` and `grep` under their own
    names. Two subsystems, two honest answers, both visible on one card.
  - Reduced motion swaps the animation for the app's own still prop, because an
    animated WebP cannot be paused from CSS and six looping robots is a lot of
    movement for someone who asked for less.

- **The kiosk family is one site.** All seven pages — info · omarchy · agents ·
  tv · global · gamba · start-crawl — now carry the same head furniture and a
  shared strip that names every other kiosk, so `tv`, `global` and `gamba`
  stop being places you can reach and not leave.
  - **A real favicon.** `favicon.ico` (16/32/48) and `favicon.svg`, cut down
    from the app's own CRT mark until it survives 16px: the tube silhouette
    and the three phosphor panes, without the stand, the scanlines or the
    eight lines of text that are a grey smudge at that size. `/favicon.ico`
    stops answering 404 on every page load.
  - **The theme travels.** The palette table moved out of `omarchy.html` into
    `assets/kiosk-theme.js`, and the pick is remembered under one key for the
    whole family — choose gruvbox on the Omarchy kiosk and the info page and
    the agent wall are already wearing it. `?t=<theme>` still opens a page
    already dressed.
  - **The cabinets keep their cabinets.** A console television that turns
    tokyo-night is no longer a console television, so on `tv`, `global`,
    `gamba` and `start-crawl` the roles are written onto the strip alone. It
    also keeps GAMBA's own `--red`, which a document-level repaint would have
    silently replaced with whatever red the palette shipped.
  - Social cards and a content-security policy on every page, not just the two
    newest.
- **`scripts/verify-kiosks.mjs`** — 104 assertions over the family: a clean
  console on every page, head parity, the strip's links, the pick surviving a
  navigation, the cabinets *not* repainting, the wall's bars agreeing with its
  verdicts, and no horizontal overflow at six widths down to 390px. It asserts
  on computed style and the console rather than on screenshots, because the
  defect it was written for was invisible in a screenshot.

### Changed

- **The agent wall shows what the app shows.** Each card carries the three bars
  read from that agent's own transcript — CTX WINDOW, FATIGUE, RELEVANCE — and
  the call they add up to, replacing the MODEL and EFFORT boxes with one
  `OPUS · MAX` chip. The verdict ladder mirrors `scripts/td-agent-vitals.mjs`,
  so a bar cannot read calm while the chip beside it says to act, and
  RELEVANCE diverges: high relevance is the good case on a roomy window and the
  alarming one on a full one. Codex panes draw no bars, which is the honest
  state of `#279` rather than an invented number.
- The wall's screenshot on the info page was two generations stale — it still
  showed the pre-card row layout and an effort gauge beside a model box that
  never once displayed a model. Regenerated from the live kiosk, and at half
  the file size.

- **An Omarchy kiosk.** `omarchy.html` joins the kiosk family (info · agents ·
  tv · global · gamba · start-crawl) and tells the desktop-integration story the
  README has been carrying alone: the two-shelf paint overlay, the eleven shared
  variant names, `SUPER+ALT+T` window adoption, Quickshell-rendered agent
  notifications, and the three-repo topology with `td-tint` as the seam.
  - The page **wears the themes it describes.** All 23 Omarchy schemes installed
    on a stock box are inlined as their named roles, and picking one repaints
    the whole page through the same role→slot mapping a painted pane uses —
    including the light schemes. `?t=<theme>` opens it already wearing one, and
    the choice is remembered.
  - The **agent-badge strip** is documented with the real mascot art and the
    real timings: the HEY blinker's 700 ms square wave and the 1.4 s bounce-eased
    breath, so NEEDS INPUT / WORKING / DONE / BLOCKED read on the page the way
    they read on the mother bar.
  - The page is built on a **scroll-driven backdrop and glass chrome**. The
    repo's own `crt-wall` screenshot sits on a fixed layer that drifts at 16% of
    scroll and scales to 1.30 across the page, blurred so it reads as phosphor
    bloom rather than as legible terminal text competing with the headline.
    Buttons, chips, cards and panels are frosted glass over a translucent tint
    of the theme's own surface colour, so the moving wall is visible *through*
    the chrome. The scrim opens at the hero and closes for prose, re-weighting
    itself for the light palettes; `prefers-reduced-motion` parks the backdrop
    and never arms the loop.
  - **The backdrop is the desktop's, and it changes as you scroll.** Every stop
    on the page — hero, seven sections, footer — names an Omarchy theme, and
    crossing into one cross-fades that theme's own shipped wallpaper onto the
    same fixed layer while the palette repaints: retro-82 → miasma →
    everforest → ethereal → tokyo-night → osaka-jade → catppuccin-latte →
    gruvbox. Colour and wallpaper are one pick on Omarchy, so they are one pick
    here. Each stop also rolls into focus through a central band (opacity,
    scale, a light blur) and the hero drifts off above it. Eight wallpapers
    cost 504KB after a 1600px cap and a webp re-encode, and each is fetched a
    screen before it is needed. The rail carries seven curated themes rather
    than all 23 — gruvbox, osaka-jade, tokyo-night, ethereal, everforest,
    miasma, retro-82 — laid out as a wrapping grid that never scrolls
    sideways, with a standing "follow scroll" toggle that says whether the
    scroll or a pick owns the page. `prefers-reduced-motion` still wears each
    stop's theme while nothing moves, and inside the last half-screen of the
    document every stop returns to full focus, so the bottom of the page is
    sharp at any viewport height.
  - **The MCP control surface gets its own section, second from the top** —
    `list_panes`, `pane_events`, `grep`, `get_pane_config`, `set_pane_config`
    and the push notifications, each with what it returns, plus the master
    switch and the appearance-only line. The nav gained an MCP entry.
  - **The theme picker scatters and reassembles as you scroll.** Two rails
    exist at once and one scrubbed number runs them in opposition: the hero's
    chips disperse on golden angles, blurring in proportion to their distance
    from home, while the ride-along's arrive from the right and converge into
    place. Both take clicks the whole way through — each chip sits in a slot
    that never moves and carries the handler, so the pill wherever it has flown
    to and the place it belongs are two targets for one action. Replacing the
    old threshold-and-DOM-move with a scrubbed value also fixed a restore that
    only worked sometimes: eight of eight returns to the top now land home.
  - **The theme picker rides along.** Past the hero on a window wider than
    1250px the rail docks to the right-hand gutter, vertically centred, as a
    narrow glass column carrying the seven themes and the follow-scroll
    toggle; scrolling back up returns it to its place under the headline. It
    is sized to the gutter (158px between 1250 and 1620, 214px above that) so
    it never covers the content column, and docking moves the node out of the
    hero — a transformed or filtered ancestor would otherwise make itself the
    containing block for a fixed child — while the vacated slot holds its
    height so the page cannot shorten under the reader.
  - **Figures open in a lightbox.** A screenshot reduced to a column width is
    unreadable; click, Enter or Space opens it as large as the window allows,
    and Escape, the backdrop or the ✕ closes it with focus returned.

- **Super+Ctrl-click reveals a path in the file manager**, where Shift- or
  Ctrl-click opens it. A pane full of printed paths — an agent's Links table,
  a build log — provokes two different questions, and only one of them was
  answerable by a click. The file manager comes up with the item *selected*
  (`org.freedesktop.FileManager1.ShowItems`, which Nautilus, Dolphin, Nemo,
  Thunar and PCManFM all export); a desktop exporting no such manager gets the
  containing directory opened instead. Works on a bare path and on a `file://`
  URI, percent-escapes and all, so a wrapped Links-table row reveals from the
  same click that opens it. The right-click menu carries the same action as
  **Reveal in folder**, shown only when the link names something on this disk.

### Changed

- **Opened links are scoped to the desktop, not to the terminal**, on a
  uwsm-managed session (Omarchy's). `xdg-open` now runs through `uwsm-app --`
  when `wayland-wm-app-daemon`'s socket is present, which is how the rest of
  such a session launches apps: the PDF a click opens gets its own systemd
  scope under `app.slice` instead of living inside the terminal's cgroup.
  Sessions without that daemon — GNOME, KDE, X11, a bare compositor — spawn
  exactly as before.

- **Paint with the desktop's own palettes.** The paint overlay now carries TWO
  shelves, turned with `z` (`shift+z` back) or by clicking a pill: the existing
  COLOUR SETS, and DESKTOP PALETTES — every Omarchy theme installed on the
  machine (`/usr/share/omarchy/themes`, `~/.config/omarchy/themes`, user themes
  shadowing stock ones by name; 23 on a stock box).
  - A palette replaces the pane's whole colour table and leaves its **texture**
    — scanlines, bloom, curvature, font — untouched, so a pane can match every
    other window on the desktop without giving up the look. It rides the same
    seam `$TD_PALETTE` already used, so there is one set of rules for both.
  - The ANSI mapping is **copied from Omarchy's own terminal template**, not
    invented: normal black is `background`, bright black is `muted`, cursor is
    `bright_foreground`. A pane painted `tokyo-night` therefore renders colour
    for colour like alacritty, foot and ghostty do under the same theme.
  - Each tile previews the scheme as a **miniature screen** in its own
    background carrying three of its own hues — a name alone cannot tell gruvbox
    from everforest. Light schemes wear a ☀ and sort last.
  - The keyboard survives the second shelf: the desktop's names collide
    (`catppuccin` beside `catppuccin-latte`, three `r`s), so a **letter cycles**
    through the palettes sharing it, painting each on the way past. `d` still
    means desktop, `esc` still folds. `z` may be a verb because nothing on
    either shelf is spelled with one — guarded by a test.
  - The pick persists per pane like any other paint pick; a palette that is
    later uninstalled falls back to the theme's own colours rather than failing.

- **The paint overlay plays from the keyboard, and the colour sets are the
  desktop's.** `terminal-delight ctl paint on` (the Omarchy 🎨 bar widget, or
  any script) still raises the palette over every pane at once — but now it is
  mouse-optional and reads like Omarchy's own picker:
  - the **focused pane is spotlit** — a thin scrim and a bright frame on it, a
    heavy scrim on everything else, so which terminal you are painting is
    answered from across the room;
  - **bare arrows** walk the wall in the direction you press (`ctrl` keeps its
    word-jump; the overlay is modal, so the plain keys are free);
  - a set's **first letter paints it**, drawn the way it is pressed — bigger,
    bolder, underlined in the accent — so the chord is legible from the tile
    instead of a legend elsewhere. `d` hands the pane back to the desktop,
    `esc` folds. A miss is a no-op, never a keystroke into the agent behind it.
- **Per-directory default logos — persistent and inherited.** Picking a pane
  logo now writes a directory default to
  `~/.config/terminal-delight/dir-logos.toml`: every pane whose cwd is at or
  under that directory wears the logo, across sessions, live as you `cd`
  (2s sweep). Mapping a child dir overrides its parent for that subtree; the
  picker says which directory a pick will bind ("↵ sets the default logo for
  ~/proj + subdirs") and its ✕ row clears the rule that currently applies. An
  explicit per-pane logo (MCP `set_pane_config`, or one saved by an older
  session) still shadows the map for that pane.

- **Alt+V / Alt+H split chords.** One-hand alternatives to `ctrl+alt+r` /
  `ctrl+alt+d`: `alt+v` opens the new pane beside the focused one (vertical
  divider), `alt+h` below it — Tilix-style naming. Listed in the `?`/F1 help
  panel. Costs readline's `alt+v` (page-scroll) and `alt+h` (mark-paragraph)
  in the shell, matching how `alt+r` was already claimed for the FOCUS reader.

- **Text-crawl mode** — a per-pane toggle that renders the whole terminal as a
  Star-Wars-style opening crawl: every line in the bundled News-Gothic crawl face
  (News Cycle, SIL OFL), centred and receding into the distance. The perspective
  is a GPU pre-map baked into the same CRT post-pass that curves the glass, so it
  composes for free with the barrel warp, screen jiggle, tracking band, glare and
  phosphor at one extra `pow` per pixel — no second render pass. Two per-pane
  dials in the DISPLAY tray (rides the grade group like warp/tracking): **angle**
  (2–30°, side convergence) and **depth** (0.05–15×, bottom-to-top text-height
  ratio). The 👓 FOCUS reader inherits the crawl font + centring (flattened for
  readability). Renderer change ships via `docs/patches/0003-text-crawl.patch`.
  Web tributes: a `start-crawl.html` kiosk and an `info.html` section.
  - _Known limitation:_ click hit-testing in a crawling pane stays barrel-only,
    so text selection is approximate — crawl is a display/nostalgia mode.

### Changed

- **One palette vocabulary, shared with the desktop.** The colour-set tray and
  the paint overlay now offer the **11 variants the Omarchy theme pack ships**
  (`army badger cherry ember glacier nuclear pineapple retro tide violet wood`)
  instead of 19 TD-only names, listed alphabetically so the paint letters run in
  reading order and every one is unique. Five sets took the desktop's name for
  the same colours — `snowflake`→**glacier**, `toxic`→**nuclear**, `ocean`→
  **tide**, `bat`→**violet**, `cyberpunk`→**retro** — and the old `retro` set
  (the slot-machine palette) is now **gamba**, after the theme it has always
  coloured. Renames are display-only: saved themes serialise the variant, not
  the label, so nothing you already picked moves. Sets no longer listed
  (`greenworks bolt amber gamba cotton-clowndy midnight retro-sunset galaxy`)
  still load from saved state — they are simply not offered.

### Fixed

- **Tabs no longer scrunch against the header icons.** The tab strip shared the
  mother bar's top line with the brand and was capped at 55% of its width, so
  four ordinary tab titles were already enough to fold it into a narrow column
  jammed beside the 🎨/📊/🤖 icons — unreadable at a glance and worse with every
  tab added. Tabs now get a ROW OF THEIR OWN beneath the brand, with the whole
  bar width: the common case doesn't wrap at all, and when it eventually does it
  grows downward without moving the brand or the controls.

- **Paint tiles with a two-word name folded mid-word.** With the desktop's own
  palettes on the second shelf the captions got longer (`catppuccin-latte`,
  `last-horizon`, `matte-black`), and a tile that wrapped where the text ran out
  read as `CATPPUCCIN-L / ATTE` and left the grid rows at ragged heights. Names
  now break on the **hyphen** onto a second line, in fixed-height boxes, so every
  tile is the same size whether its name takes one line or two.

- **Logo picker missed most of the filesystem.** The candidate scan walked the
  home root only 2 levels deep, so project brand assets
  (`~/ORG/Software/<proj>/assets/logo.png`) never appeared — only the picture
  dirs did. The walk is now full-depth (bounded by a 20k cap + heavy-dir skip
  list; picture dirs still scan first), and `.webp` counts as an image.

## [0.2.0] — 2026-06-15

The "now you can actually download it" release: a single MIT-licensed AppImage,
no more source-only. Plus a per-pane agent-finished bell and richer agent panes.

### Added

- **Prebuilt, MIT-clean AppImage.** `scripts/build-appimage.sh` produces a single
  self-contained `terminal-delight-x86_64.AppImage`, bundling a `cargo about`
  third-party license notice; CI builds it on every `main` push and **attaches it
  to the GitHub Release on version tags** (one-command install). Graphics libraries
  are loaded from the host (a GPU app must use the host driver stack).
- **Per-pane agent bell.** When a program rings the terminal bell (BEL) — as
  Claude/Codex do when they finish — the pane plays a configurable sound (trimmed
  clip, optional loop), raises a SNOOZE bar, and shows an always-visible `♪` mute.
  Five PD/CC0 default sounds are bundled and seeded on first run; playback is via
  the host `ffplay` (degrades silently without `ffmpeg`). See `BELL_SOUNDS.md`.
- **Agent panes.** Your own messages get their own colour (👤 colour-wheel pip) and
  `Alt+↑/↓` / ▲▼ jump between them; an agentic help section in the `?` modal.
- **Portability hardening** (toward running on untested boxes — AMD/Intel,
  Wayland, fractional scaling): vendor-agnostic GPU check in `scripts/setup-deps.sh`;
  an explicit monospace **font fallback chain** with a startup diagnostic when the
  default isn't installed (no more silent substitution); a startup log of the
  wgpu **GPU/driver** gpui selected; and **X11 PRIMARY-selection** copy
  (select-to-copy + write-on-copy, so middle-click paste works in other apps).
- Right-click context menu (Copy / Paste / Open link); `?` help modal; a split now
  inherits the seed terminal's working directory.

### Changed

- **Binaries are now MIT-distributable** — the project is **no longer source-only**.
  `docs/patches/0002-sever-gpl-crates.patch` removes the GPL-3.0 crates (`ztracing`,
  `zlog`, `ztracing_macro`) that the Zed graph linked via `gpui -> sum_tree`; they
  were trace-only. `app/deny.toml` now passes with **no GPL exceptions**.
  `scripts/prepare-gpui.sh` applies both patches.

## [0.1.0] — 2026-06-14

First public, source-only release. A GPU-native Linux terminal (Rust + gpui +
`alacritty_terminal`) with a hot-reloadable, CRT-flavored visual identity.

### Added

- **Real terminal core.** PTY + full VT emulation (bash, vim, top, tmux
  verified); live resize → SIGWINCH; full ANSI colour (16 themed + 256 +
  truecolor), bold/underline/inverse/dim; scrollback, mouse selection, copy/paste
  with bracketed paste.
- **Tiling multi-pane.** True tiling-tree splits (`ctrl+alt+r` / `ctrl+alt+d`)
  that divide only the focused pane, tab strip, `alt+←/→` focus movement, sub-tab
  drag-to-split/move, and a pop-out scratch window with sub-tab tear-off.
- **Hot-reloadable themes.** Four built-ins (`quiet-command`, `field-command`,
  `tactical-overdrive`, `hacker`) plus a live-editable `custom` slot read from
  `~/.config/terminal-delight/theme.toml` and reloaded on save (~300 ms). Theme
  picker with per-glyph captions and 1.5 s hover tooltips; the custom slot's
  tooltip shows its resolved path and an "Open in editor" action.
- **Per-pane appearance.** A pane's look splits into two independently-inheriting
  groups — the theme group (theme/seed/colour-mode/syntax) and the monitor-OSD
  grade group — each with a live, non-destructive "follow outer" toggle.
- **Monitor-OSD grading.** A display tray (global or per-pane) with
  brightness / contrast / colour / text / background / gamma, applied in HSLA at
  paint time, **plus a text-size channel** that rides the same inherit/override
  scope.
- **Seed colour wheel** for retinting a theme from a single accent colour.
- **CRT-lite effects** — scanlines, vignette, glow, and a per-pane barrel warp
  via the vendored `td-crt-pass` gpui renderer patch — all per-theme dials.
- **Desktop integration.** `scripts/install-hotkey.sh` registers
  `Ctrl+Alt+T` on GNOME to launch the app (reversible with `--uninstall`).

### Project / packaging

- MIT-licensed own source; binaries are **not** MIT-distributable because the
  vendored Zed/gpui graph links GPL-3.0 crates — see
  [`THIRD-PARTY-LICENSES.md`](THIRD-PARTY-LICENSES.md). This is a **source-only**
  release.
- CI gate: fmt + clippy (`-D warnings`) + tests + release build + `cargo-deny`
  (licenses/bans/advisories/sources) + browser-prototype checks.
- Contributor docs: [`CONTRIBUTING.md`](CONTRIBUTING.md), issue/PR templates,
  [`SECURITY.md`](SECURITY.md), [`CODE_OF_CONDUCT.md`](CODE_OF_CONDUCT.md).

### Platform

- Linux only (X11 & Wayland via gpui's wgpu renderer). Not macOS/Windows.

[Unreleased]: https://github.com/parker-brown-family/terminal-delight/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/parker-brown-family/terminal-delight/releases/tag/v0.2.0
[0.1.0]: https://github.com/parker-brown-family/terminal-delight/releases/tag/v0.1.0
