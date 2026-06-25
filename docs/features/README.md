# Terminal Delight — Feature Catalog

> The terminal you watch your fleet through. A GPU-native Linux terminal that
> doubles as **mission control for a fleet of coding agents** — read-only, retro,
> and fast.

This catalog is the canonical, evidence-backed inventory of what Terminal Delight
does, organised so we can draw from it for the site, the README, release notes,
and GTM. Every entry cites the source (`file:line`), any `TD_*` flag or keybinding,
and ship status. **85+ features across 9 capability areas.**

## The one-line pitch

A real, fast terminal (Rust + GPU, Alacritty-comparable latency) with a
**read-only agent-observability layer** bolted on: an MCP control surface that lets
an orchestrator *watch* every agent pane, an in-app **agent wall** dashboard, an
**agent graveyard** to resurrect dead sessions, and a plugin surface — all wrapped
in a genuinely beautiful CRT aesthetic with per-pane themes and barrel-warp glass.

## Read in this order (the v1 story)

1. **[Agentic / MCP monitoring](01-agentic-mcp.md)** — ⭐ **the headline.** Watch
   your agents' transcripts and state through a read-only MCP surface. This is the
   hook; everything else supports it.
2. **[Agent dashboard / wall](02-agent-dashboard.md)** — the in-app HUD: grouped
   cards, live state/effort/tokens, needs-you highlighting, per-card theme & art.
3. **[Agent graveyard / recover](03-agent-graveyard.md)** — find dead agent
   sessions on disk and one-click resurrect them.
4. **[Plugins](04-plugins.md)** — the plugin surface + the LeanCTX token-savings
   plugin (backlinks [leanctx.com](https://leanctx.com/)) + context-delight harvest.
5. **[Visuals / CRT](05-visuals-crt.md)** — true post-process barrel warp, the 👓
   FOCUS reader with GPU blur, text-crawl, phosphor/scanlines/glare.
6. **[Themes](06-themes.md)** — hot-reload colour-sets, per-pane independence,
   monitor-OSD grade sliders, seed colour wheel, syntax schemes.
7. **[Terminal core](07-terminal-core.md)** — tabs, tiling, session restore,
   find/search, selection, frameless window, latency.
8. **[Internationalization](08-i18n.md)** — the whole UI in 9 languages,
   compiler-enforced completeness.
9. **[Packaging / platform](09-packaging.md)** — MIT AppImage, GPL severed, the
   gpui fork, portability hardening.

Plus: **[ENV-FLAGS.md](ENV-FLAGS.md)** — every `TD_*` environment flag in one table.

## Status legend

- **Shipped** — in `main`, tested, in the released binary.
- **Partial** — works, with a documented gap or demo-gated.
- **Roadmap** — designed, not yet built (tracked in the GitHub `1.0` milestone).

## The positioning line (keep it honest)

Terminal Delight is the **HUD over your agent fleet** — observability and craft,
**not an orchestrator.** Orchestration lives upstream (your harness); TD is where
you *watch* the agents work, beautifully. That boundary is deliberate and is what
keeps the product focused.
