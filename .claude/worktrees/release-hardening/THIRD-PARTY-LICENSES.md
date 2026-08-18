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
