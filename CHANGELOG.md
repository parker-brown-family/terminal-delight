# Changelog

All notable changes to terminal-delight are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project aims
to follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html) once it
reaches 1.0. Until then, `0.x` minor bumps may include breaking changes.

## [Unreleased]

### Added

- **Paint with the desktop's own palettes.** PAINT mode now carries two shelves,
  cycled with `z` (`shift+z` back) or by clicking the pill: the existing COLOUR
  SETS, and DESKTOP PALETTES — every Omarchy theme installed on the machine
  (`/usr/share/omarchy/themes`, `~/.config/omarchy/themes`, user themes shadowing
  stock ones by name). A palette replaces the pane's whole colour table and
  leaves its texture — scanlines, bloom, curvature, font — untouched, so a pane
  can match every other window on the desktop without giving up the look. The
  ANSI mapping is copied from Omarchy's own terminal template, not invented, so
  a pane painted `tokyo-night` renders colour for colour like alacritty, foot and
  ghostty do under the same theme. Each tile previews the scheme as a miniature
  screen in its own background; light schemes carry a ☀ and sort last. The pick
  persists per pane like any other paint pick, and a palette that is later
  uninstalled falls back to the theme's own colours rather than failing.

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

### Fixed

- **Tabs no longer scrunch against the header icons.** The tab strip shared the
  mother bar's top line with the brand and was capped at 55% of its width, so
  four ordinary tab titles were already enough to fold it into a narrow column
  jammed beside the 🎨/📊/🤖 icons — unreadable at a glance and worse with every
  tab added. Tabs now get a ROW OF THEIR OWN beneath the brand, with the whole
  bar width: the common case doesn't wrap at all, and when it eventually does it
  grows downward without moving the brand or the controls.

- **Paint tiles with long names folded mid-word.** `RETRO-SUNSET` rendered as
  `RETRO-SUN / SET` and `GREENWORKS` as `GREENWORK / S`, and the ragged captions
  left the grid rows staggered at different heights. Captions now break on the
  hyphen (`RETRO / SUNSET`) in fixed-height boxes, so every tile is the same size
  whether its name takes one line or two.

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
