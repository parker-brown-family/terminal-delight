# Third-Party Licenses

terminal-delight's own source is MIT. Its direct UI/terminal dependencies —
`gpui`, `gpui_wgpu`, `gpui_platform`, `gpui_linux`, and `alacritty_terminal` —
are Apache-2.0. Rust dependency licenses are checked in CI with
`cargo deny check licenses` against the allowlist in `app/deny.toml`.

## Source vs. binary distribution

This repository is **source-only**. The pinned Zed dependency graph links
**GPL-3.0-or-later** crates — `ztracing`, `ztracing_macro`, and `zlog` — into the
built binary through `gpui -> sum_tree`. Consequently:

- **Source distribution** under MIT is fine: the GPL crates are *not* redistributed
  here. You build them yourself from your own Zed checkout (`scripts/prepare-gpui.sh`),
  so this tree carries only terminal-delight's MIT code.
- **Binary distribution would be a derivative work** of the GPL-3.0-or-later crates
  and must therefore be licensed GPL-3.0-or-later (with corresponding source).
  Do not publish prebuilt binaries under MIT.

Unlocking MIT binaries later means either relicensing terminal-delight to
GPL-3.0-or-later, or severing the `sum_tree -> ztracing` edge in the gpui fork.

## Attribution bundle for any binary release

Apache-2.0 (§4) and the other dependency licenses require carrying their notices.
Before publishing any binary artifact, generate the full third-party notice bundle
from the locked dependency graph and ship it alongside the binary:

```bash
cd app
cargo about generate about.hbs > ../THIRD-PARTY-LICENSES.generated.html
```

The generated file is intentionally not hand-maintained.
