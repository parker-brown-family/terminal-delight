# Packaging / platform — an MIT binary you can ship

Terminal Delight ships as a single-file, **MIT-licensed** AppImage — no GPL
obligations, no driver bundling, GPU-agnostic. Getting there required severing a
GPL edge in the dependency graph and a small, tracked fork of gpui's renderer.

## Why it matters

"Open & shareable" is a 1.0 bar: one-command install, portable themes, a binary you
can redistribute cleanly. The licence story is deliberate and documented so there
are no surprises.

## Features

| Feature | What it does | Evidence | Status |
|---|---|---|---|
| **MIT binary** | GPL crates (`ztracing`/`zlog`) severed from the graph; `cargo deny` passes with **no** GPL exceptions | `docs/patches/0002-sever-gpl-crates.patch`, `app/deny.toml` | Shipped |
| **Prebuilt AppImage** | `chmod +x` and run; built on `main`, tagged `v*` builds + attaches to the Release | `scripts/build-appimage.sh` | Shipped |
| **GPU portability** | wgpu renderer (not blade) → NVIDIA/AMD/Intel, X11 + Wayland; startup self-reports GPU/driver | gpui wgpu (Zed PR #46758) | Shipped (broad) |
| **Font fallback + diagnostics** | Explicit monospace fallback chain; loud warning if the default is missing | startup log | Shipped |
| **Licence boundary docs** | `THIRD-PARTY-LICENSES.md` via cargo-about; source-vs-binary split documented; clean-room rule (Zed terminal crates study-only) | README, PLAN.md | Shipped |
| **cargo-deny policy** | CI fails on any new copyleft dep | `deny.toml` | Shipped |
| **Build from source** | `prepare-gpui.sh` clones the pinned Zed rev + applies the CRT patches; then `cargo run/release` | `scripts/prepare-gpui.sh` | Shipped |
| **Patch management** | The fork is 5 data-file patches (`docs/patches/0001-0004` + GPL sever) on a pinned rev; rebase cost is explicit | `docs/patches/` | Shipped |
| **Flatpak** | Second distributable alongside AppImage | — | Roadmap (#140) |

## The licence boundary (one paragraph)

Repo is **MIT**; gpui + gpui_wgpu + alacritty_terminal are **Apache-2.0** (gpui is
*not* GPL — common misconception). The distributed **binary** is MIT because the GPL
trace-only crates were severed (`0002`); source builds are clean either way. Zed's
own `crates/terminal` is GPL — used as **shape reference only, never copied**
(clean-room).

## Build substrate

gpui is consumed via path deps to a pinned `zed-upstream` checkout (rev `abbe85a`)
carrying the wgpu Linux renderer — **not** crates.io `gpui` (which ships the blade
renderer, broken on NVIDIA/X11). The CRT shaders live as patches on that checkout.

## Status

**Shipped** for AppImage/Linux. Flatpak + a broader Linux validation matrix
(AMD/Intel, Wayland, fractional scaling) are the open 1.0 items (#139, #140).
