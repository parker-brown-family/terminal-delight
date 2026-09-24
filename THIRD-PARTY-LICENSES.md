# Third-Party Licenses

terminal-delight's own source is MIT, and so are its distributed binaries. Its
direct UI/terminal dependencies — `gpui`, `gpui_wgpu`, `gpui_platform`,
`gpui_linux`, and `alacritty_terminal` — are Apache-2.0, and every transitive
dependency is used under a permissive license (MIT / Apache-2.0 / BSD-class /
Zlib / ISC / MPL-2.0 / Unicode / CC0). Rust dependency licenses are checked in CI
with `cargo deny check` against the allowlist in `app/deny.toml`, with **no GPL
exceptions**.

## Vendored: Omarchy's agent usage collectors (MIT)

`app/src/vendor/omarchy-agent-usage-{claude,codex,fireworks}` are byte-identical
copies of three scripts from **Omarchy** (<https://github.com/basecamp/omarchy>,
MIT), taken from package version `4.0.1-1` on 2026-09-02. They are compiled into
the binary by `app/src/usage.rs` and run by `terminal-delight agent-usage update`
to publish one usage record per AI coding subscription.

They are Python 3 **stdlib-only** scripts, so they add nothing to the linked
dependency graph and change none of the analysis above — the binary stays MIT.
They are carried unedited, which makes `cmp` against an installed Omarchy the
whole audit; see `app/src/vendor/README.md` for provenance and the re-sync
recipe. Running them requires `python3` at runtime, which is a soft dependency:
without it the usage panel says so and still draws whatever records are on disk.

## Lifted: markdown-delight's Markdown renderer (MIT)

`app/src/docview/markdown.rs` draws Markdown inside a pane, in a floating square
and on the workbench's Markdown cards. It is lifted from **markdown-delight**
(<https://github.com/parker-brown-family/markdown-delight>), `app/src/render.rs`
with `BlockMeta`, `normalize` and `fingerprint` from `app/src/comments.rs`, at
commit `425041bd36d31cd44d25984ee9cd71d071ead948`. Unlike the Omarchy scripts it
is **edited**, not carried verbatim: its colours come from TD's theme instead of
fixed constants, it keeps link targets and draws local images, and the file's
header lists every change. markdown-delight's licence:

```
MIT License

Copyright (c) 2026 Parker Brown / brown-family-sports

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

## comrak, and what it brings (BSD-2-Clause and permissive)

The renderer parses with **comrak** 0.52 (<https://github.com/kivikakk/comrak>,
BSD-2-Clause), built with `default-features = false`: its defaults add comrak's
own command line and syntax highlighter (clap, syntect and the onig C library),
which TD does not use. So built, comrak adds eight packages to the lock:
`comrak` (BSD-2-Clause), `caseless`, `entities`, `phf_codegen` and `typed-arena`
(MIT), `jetscii` and `unicode-normalization` (MIT OR Apache-2.0), and
`finl_unicode`, whose licence is `(MIT OR Apache-2.0) AND Unicode-DFS-2016` for
its Unicode data tables. The Unicode data licence is permissive and
OSI-approved; `app/deny.toml` grants it to `finl_unicode` by name, as it already
does for `unicode-ident`, rather than to every crate. Their notices ride in the
generated bundle below like every other dependency's.

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

## Bundled fonts

Two typefaces are compiled into the binary via `include_bytes!` (so they ship
inside the AppImage too), because neither can be assumed installed and both are
load-bearing for a mode that would otherwise silently fall back to the UI sans:

| File | Family | License | Attribution |
|------|--------|---------|-------------|
| `app/assets/fonts/NewsCycle-Bold.ttf` | News Cycle | **SIL Open Font License 1.1** (`app/assets/fonts/OFL.txt`) | © 2010–2011 Nathan Willis, with Reserved Font Name "News Cycle". A libre News-Gothic-class face — the closest freely-licensable match to the crawl's News Gothic typeface. Unmodified. |
| `app/assets/fonts/Caveat-Variable.ttf` | Caveat | **SIL Open Font License 1.1** (`app/assets/fonts/OFL-Caveat.txt`) | © 2014 The Caveat Project Authors (`github.com/googlefonts/caveat`), with Reserved Font Name "Caveat". The handwriting the sticky notes are written in. Upstream variable (`wght`) build, unmodified. |

The full OFL text travels beside each font — `app/assets/fonts/OFL.txt` for News
Cycle, `app/assets/fonts/OFL-Caveat.txt` for Caveat.
