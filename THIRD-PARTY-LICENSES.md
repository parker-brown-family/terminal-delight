# Third-Party Licenses

terminal-delight's own source is MIT, and so are its distributed binaries. Its
direct UI/terminal dependencies — `gpui`, `gpui_wgpu`, `gpui_platform`,
`gpui_linux`, and `alacritty_terminal` — are Apache-2.0, and every transitive
dependency is used under a permissive license (MIT / Apache-2.0 / BSD-class /
Zlib / ISC / MPL-2.0 / Unicode / CC0). Rust dependency licenses are checked in CI
with `cargo deny check` against the allowlist in `app/deny.toml`, with **no GPL
exceptions**.

## No copyleft in the binary

The pinned Zed dependency graph *would* otherwise link three
**GPL-3.0-or-later** crates — `ztracing`, `ztracing_macro`, and `zlog` — into the
binary through `gpui -> sum_tree`. They were used only for trace-span attributes
and a test-logger init. `docs/patches/0002-sever-gpl-crates.patch` removes those
uses and drops the dependencies, so they never reach the compiled binary.

With that edge severed:

- **Source distribution** is MIT (the GPL crates were never in this tree anyway).
- **Binary distribution** (e.g. the prebuilt AppImage) is also MIT-compatible:
  the linked graph is entirely permissive, so a distributed binary carries no
  copyleft obligations.

A couple of dependencies are dual/tri-licensed with a copyleft *option*
(`self_cell` = `Apache-2.0 OR GPL-2.0-only`; `r-efi` = `MIT OR Apache-2.0 OR
LGPL-2.1-or-later`); terminal-delight elects the permissive arm, exactly as
`cargo deny`/`cargo about` resolve them.

## Attribution bundle shipped with binaries

Apache-2.0 (§4) and the other dependency licenses require carrying their notices.
`scripts/build-appimage.sh` generates the full third-party notice bundle from the
locked dependency graph and ships it inside the AppImage at
`usr/share/licenses/terminal-delight/THIRD-PARTY-LICENSES.txt`:

```bash
cd app
cargo about generate about.hbs > THIRD-PARTY-LICENSES.txt
```

The generated bundle is intentionally not hand-maintained; `app/about.toml`
configures it (accepted licenses mirror `deny.toml`, scoped to the Linux target).

## Bundled audio (agent-bell sounds)

The default agent-bell clips in `app/assets/sounds/` (also seeded into
`~/.config/terminal-delight/sounds/` and bundled in the AppImage) are recordings
of public-domain compositions. All are format-converted to mp3 and some are
length-capped; the underlying compositions are public domain.

| File | Source recording | License | Attribution |
|------|------------------|---------|-------------|
| `alert.mp3` | generated two-tone chime | public domain (original) | — |
| `fate.mp3` | Beethoven, Symphony No. 5, i. — Musopen, via Wikimedia Commons | **Public domain** | — |
| `moonlight.mp3` | Beethoven, Sonata No. 14, i. — Musopen, via Wikimedia Commons | **Public domain** | — |
| `bald-mountain.mp3` | Mussorgsky, Night on Bald Mountain — Musopen, via Wikimedia Commons | **Public domain** | — |
| `fur-elise.mp3` | "Fur Elise.ogg", Wikimedia Commons | **CC BY-SA 3.0** (https://creativecommons.org/licenses/by-sa/3.0) | © Wikimedia Commons user **Sebion7125**; converted to mp3. Shared under the same CC BY-SA 3.0. |

`wild-eep.mp3` (classic Mac OS alert) is **Apple-owned and never bundled or
committed** — it exists only in a user's local sounds dir for personal use.

## Bundled fonts (text-crawl mode)

The Star-Wars-style **text crawl** mode renders the terminal in a bundled
typeface, compiled into the binary via `include_bytes!` (so it ships inside the
AppImage too):

| File | Family | License | Attribution |
|------|--------|---------|-------------|
| `app/assets/fonts/NewsCycle-Bold.ttf` | News Cycle | **SIL Open Font License 1.1** (`app/assets/fonts/OFL.txt`) | © 2010–2011 Nathan Willis, with Reserved Font Name "News Cycle". A libre News-Gothic-class face — the closest freely-licensable match to the crawl's News Gothic typeface. Unmodified. |

The full OFL text travels beside the font at `app/assets/fonts/OFL.txt`.
