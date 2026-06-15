# Changelog

All notable changes to terminal-delight are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project aims
to follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html) once it
reaches 1.0. Until then, `0.x` minor bumps may include breaking changes.

## [Unreleased]

_Nothing yet._

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
