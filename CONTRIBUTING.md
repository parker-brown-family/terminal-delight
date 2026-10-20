# Contributing to terminal-delight

Thanks for being here. There are two ways in, and you do **not** need to write
Rust for the first one.

1. **Themes** — author a `.toml`, no compiler required. This is the wide path.
2. **Code** — the Rust app (`app/`) and the gpui renderer patch.

First, one rule that shapes everything:

## Licensing, and where binaries come from

terminal-delight is **MIT**, and so is the AppImage it ships. The pinned Zed
dependency graph would link three GPL-3.0-or-later crates (`ztracing`,
`ztracing_macro`, `zlog`) into the binary through `gpui -> sum_tree`.
`docs/patches/0002-sever-gpl-crates.patch` removes them, `scripts/prepare-gpui.sh`
applies it, and `cargo deny check` (allowlist in `app/deny.toml`, **no GPL
exceptions**) fails CI if any copyleft crate comes back. See
[`THIRD-PARTY-LICENSES.md`](THIRD-PARTY-LICENSES.md) for the full story.

That makes two rules for contributors:

- **Keep the graph permissive.** A new dependency must pass `cargo deny check`.
  Never widen `deny.toml` to admit a copyleft licence.
- **Binaries come only from CI.** Releases are built and published by the
  workflow described in [`RELEASING.md`](RELEASING.md), from a tagged commit,
  with a recorded digest. Do not attach a binary you built to a pull request or
  an issue. Nobody can verify what it was built from, so it cannot be trusted
  or redistributed.

By contributing you agree your contribution is licensed MIT (the repo's
license).

## Changelog

A pull request that changes something a user can see or has to do adds a line
under `## [Unreleased]` in [`CHANGELOG.md`](CHANGELOG.md), in the section it
belongs to (*Added*, *Changed*, *Fixed*, *Removed*, *Security*). Describe what
someone using Terminal Delight notices, not what the code does. If it changes a
config file, a default or a key, say what an upgrading user should do.

---

## Path 1 — Themes (no Rust)

Themes are plain TOML data files. The app hot-reloads them: edit the file while
it's running and the change lands in ~300ms, no restart, no recompile.

### Try it live in 30 seconds

Your config theme lives at `~/.config/terminal-delight/theme.toml` (seeded from
`hacker` on first run). Open it, change `accent`, save — watch the running app
update. That file is the **`custom`** slot in the theme picker.

### Anatomy of a theme

Copy [`app/themes/hacker.toml`](app/themes/hacker.toml) as a starting point.
Every field:

```toml
name = "midnight"          # registry id / identity
icon = "☾"                 # the glyph that stands in for the theme in the picker

[colors]
bg      = "#03100a"        # window background
surface = "#071a10"        # panels, headers, tray
text    = "#86efac"        # default foreground
accent  = "#22c55e"        # focus borders, cursor glow, highlights
faint   = "#14401f"        # dim chrome, inactive borders
cursor  = "#4ade80"        # optional; defaults to a lightened accent
ansi    = [ ... 16 hex ... ] # ANSI 0-15 (8 normal + 8 bright), in palette order

[effects]                  # all optional; 0 = off
scanline_opacity = 0.22    # CRT scanline darkness
scanline_step    = 4.0     # px between scanlines
vignette         = 0.8     # top/bottom falloff
glow             = 0.85    # accent glow (header, cursor)
bloom            = 0.9     # centre phosphor bloom
tracking         = 0.6     # rolling tracking-band strength
tracking_period  = 16.0    # seconds between sweeps
tracking_sweep   = 7.0
flicker          = 0.5     # stepped flicker
jiggle           = 0.7     # rare vertical-hold hop
curvature        = 0.45    # barrel warp — needs the td-crt-pass renderer patch
screen_glare     = 0.42    # top-left glass reflection
bezel            = 0.0     # raised metallic frame around each pane (0 = flat)

[font]                     # optional
family      = "JetBrains Mono"
size        = 14.0
cell_height = 20.0
```

Notes:

- **`ansi` must have exactly 16 entries** (indices 0–15). The terminal maps them
  in the standard xterm order; index 7/15 are your foreground whites.
- **`icon`** is the glyph in the picker tray. Under it the picker shows a short
  caption derived from the theme's id (the part before the first `-`, so
  `tactical-overdrive` → `tactical`). The caption is what keeps two themes that
  happen to share a glyph tellable apart — pick whatever glyph you like, the
  caption disambiguates.
- **`curvature`** only bends if you've applied the `td-crt-pass` renderer patch
  (`scripts/prepare-gpui.sh` does this). Without it the dial is a no-op.
- Want it effect-free (a clean, modern look)? Set the whole `[effects]` block to
  zeros — see [`quiet-command.toml`](app/themes/quiet-command.toml).

### Submitting a built-in theme

To ship your theme with the app:

1. Drop `app/themes/<your-theme>.toml` in the repo.
2. Register it in `BUILTIN_THEMES` in
   [`app/src/theme.rs`](app/src/theme.rs) (one `(id, include_str!(...))` line).
   Keep `name` in the file equal to the registry id — a test enforces this.
3. Run `cargo test` (the theme-parsing tests will validate your file).
4. Add a screenshot to the PR.
5. Give it a caption in the picker, in all nine languages, as
   [`docs/TRANSLATING.md`](docs/TRANSLATING.md) describes. CI's
   **Translation suggestions** check reminds you, and never blocks.

A good built-in is internally coherent (the ANSI ramp reads as one family) and
legible at a glance. Loud is fine; unreadable is not.

---

## Path 2 — Code

### Dev setup

```bash
bash scripts/setup-deps.sh     # Vulkan + build libs (Ubuntu/Debian)
bash scripts/prepare-gpui.sh   # clone the pinned Zed + apply the td-crt-pass patch
cd app && cargo run
```

`prepare-gpui.sh` clones Zed at the rev pinned in `app/Cargo.toml`
(`[package.metadata.terminal-delight] zed_rev`) into a sibling `zed-upstream/`
directory and applies `docs/patches/0001-td-crt-pass.patch` (the per-pane CRT
barrel-warp renderer pass). It's idempotent — safe to re-run.

### The bar (CI runs all of this — run it before you push)

```bash
cd app
cargo fmt -- --check
cargo clippy --locked -- -D warnings   # warnings are errors
cargo test --locked
cargo build --release --locked
cargo deny check                       # license + source policy
```

Plus, from the repo root, `node --check src/js/*.js` for the browser prototype.

Text a person reads on screen is drawn from the language pack, in nine
languages. If your change adds some, `python3 scripts/translation-suggest.py`
lists it and [`docs/TRANSLATING.md`](docs/TRANSLATING.md) says how to add it. CI
runs the same check as a suggestion; it never fails the build.

### Clean-room rule for Zed (important)

Zed's `terminal` / `terminal_view` crates are **GPL-3.0-or-later — study only,
never copy**. You may learn *architectural facts* from Zed ("wrap `Term` in an
`Arc<FairMutex>`, forward events over a channel, send `Msg::Resize` on bounds
change"). You may **not** transcribe function bodies, identifiers, or structure.
Write the terminal seam, `app/src/vt/`, from the published sources and APIs of
the cores it wraps — `rio-vt` (MIT) and the fallback `alacritty_terminal`
(Apache-2.0) — not with Zed source open. See `docs/PLAN.md` §2, and
`docs/plans/core-swap-rio/` for why the seam is shaped the way it is.

Anything that reads a terminal goes through `crate::vt`; nothing outside
`app/src/vt/` may name `rio_vt` or `alacritty_terminal`, and
`vt::boundary::no_file_outside_vt_names_a_core_crate` fails the build if one
does. Run the suite on both cores before changing the seam:
`cargo test` and `cargo test --features core-alacritty`.

### PRs

- One focused change per PR; describe the *why*.
- Include test output / a screenshot for anything user-visible.
- Keep new code in the idiom of the file around it.
- See [`docs/PLAN.md`](docs/PLAN.md) for the roadmap and where things are headed.

Questions? Open an issue — happy to help you land your first theme or patch.
