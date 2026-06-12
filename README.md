# terminal-delight

A **GPU-native Linux terminal** with a hot-reloadable, CRT-flavored visual identity.
Rust end-to-end: [gpui](https://github.com/zed-industries/zed/tree/main/crates/gpui)
(Zed's GPU UI framework) renders everything; [`alacritty_terminal`](https://docs.rs/alacritty_terminal)
does the VT emulation; your real shell runs on a real PTY.

> Goal: **2-5-20 terminals in one window · native-snappy · web-app polished ·
> modify-at-will themes · open source.** See [docs/PLAN.md](docs/PLAN.md) for the
> gated build plan (all five G0 risk gates + MVP 0.1: **passed**).

![two panes: vim + top](assets/mvp-vim-top.png)

## Status — MVP 0.1 (two-pane real terminal)

| Capability | State |
|---|---|
| Real shells (PTY + full VT emulation) — bash, vim, top, tmux verified | ✅ |
| Two split panes, per-pane grids, focus borders | ✅ |
| `ctrl+alt+r` / `ctrl+alt+d` split · `alt+←/→` switch panes | ✅ |
| Pane closes when its shell exits; last one quits the app | ✅ |
| Layout (pane count) restores on launch | ✅ |
| Live resize → SIGWINCH (verified against `tput`) | ✅ |
| Full ANSI color (16 themed + 256 + truecolor), bold/underline/inverse/dim | ✅ |
| Scrollback (wheel), mouse selection (click/word/line), `ctrl+shift+c/v`, bracketed paste | ✅ |
| **Hot-reload themes** — edit `~/.config/terminal-delight/theme.toml`, no restart | ✅ |
| CRT-lite effects: scanlines, vignette, glow — per-theme dials, fully off in light theme | ✅ |
| Latency probe (`TD_LATENCY=1`): key→echo→parsed **p50 121µs / p99 169µs**; `seq 1 100000` in **0.089s** | ✅ |

## Build & run

```bash
# deps (Ubuntu): bash scripts/setup-deps.sh   (Vulkan + build libs)
git clone --depth 1 https://github.com/zed-industries/zed ../zed-upstream  # pinned substrate
cd app && cargo run
```

gpui is consumed from a pinned zed checkout (`abbe85a`, post-wgpu-Linux-renderer —
the crates.io release still ships the older blade renderer with known NVIDIA/X11 issues).

## Theming — edit while it runs

First launch seeds `~/.config/terminal-delight/theme.toml` (hacker). Change any value —
colors, the 16 ANSI slots, `scanline_opacity`, `vignette`, `glow`, font — and the running
app picks it up in ~300ms. Four themes ship in [`app/themes/`](app/themes/):
**hacker** (phosphor green) · **tactical-overdrive** (cyan) · **field-command** (olive) ·
**quiet-command** (light, effects off). Copy one over your config file to switch.

## Architecture

```
app/src/main.rs   Workspace: panes, split/focus/close, layout persistence
app/src/pane.rs   TerminalView: grid render (styled runs), input→PTY bytes,
                  selection, scrollback, clipboard, CRT-lite, latency probe
app/src/term.rs   the seam: alacritty_terminal tty+EventLoop (clean-room, Apache-2.0 API)
app/src/theme.rs  TOML themes, hot-reload watcher, gpui Global
app/themes/       shipped themes (data files — the no-Rust contribution path)
docs/PLAN.md      the adversarially-hardened plan, gates G0a–G0e + milestones
index.html, src/  original browser design prototype (kept as design reference)
```

License: MIT. (`gpui`, `alacritty_terminal` = Apache-2.0. Zed's GPL terminal crates
were used as *shape* reference only — see the clean-room rule in docs/PLAN.md §2.)

## Roadmap

**0.2** tabs · up to 5 panes · drag splitters · packaging smoke test (AppImage/Flatpak) ·
**0.3** detach pane → own window · **0.4** true post-process CRT shader (wgpu pass — fork
gate per PLAN R1) · **1.0** 20 panes · rigorous latency rig vs Alacritty · theme gallery.
