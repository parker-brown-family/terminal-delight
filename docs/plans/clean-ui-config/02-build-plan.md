# Build plan: Clean and UI CONFIG (Gates 2–4, combined)

Parker approved Gate 1 with changes on 2026-10-22 and handed over the rest: *"Open in WS 7 when you have a build ready for me to look at play with and maybe offer feedback... but maybe you JUST SMASH IT!!!! Make it award winning!"* The three remaining gates are collapsed into this page. Least-confident decisions are listed so they can be argued with on the build itself.

## Gate 1 changes carried in

- Clean prefers **top tabs**: it hides the left bar, and the ◀/▶ handle that switches between them is never hidden.
- The **spine** is configurable (the tmux-style user hides it). Clean keeps the **attention pill**.
- **Clean never blocks**: a switch you set yourself outranks anything Clean asks for.
- Menu names: Painter → **SURFACES**, Gauges → **SLIDERS**. A pane's ⋯ lists **+ Agent, Surface, Sliders, Bench**, then the handover and Close.
- Visibility sets are **data files**, like the Chalk and OPS skins, so a set can be authored and handed to an agent as a package.

## Architecture

**A fourth axis, beside palette, finish and skin.** `app/src/uiconfig.rs` owns what chrome is drawn. A **set** is a TOML file listing switches: `[outer]` and `[panes]` tables of `key = bool`. The built-in sets are embedded from `app/ui/*.toml` (`all`, `clean`); user sets come from `~/.config/terminal-delight/ui/*.toml`. A key a set does not mention is no opinion, and in the end resolves to shown.

**One choice per window**, persisted in the session file as `[ui]`:

```rust
pub struct UiChoice {
    /// A set chosen by hand. None = follow what the DESIGN asks for.
    pub set: Option<String>,
    /// Switches flipped by hand. Outrank every set.
    pub overrides: BTreeMap<String, bool>,
}
```

Effective switch = `overrides[key]` → `set(choice.set ?? design.ui ?? "all")[key]` → shown. A design asks for a set with `ui = "clean"` in its theme file, the way `skin = "ops"` asks for a trim. This is the rule Parker asked for: Clean asks, it never blocks.

**The left bar keeps its own toggle**: Ctrl+Shift+B, ◀ and ▶. Those now write the `left_bar` override. An old session file with `left_bar = false` reads back as that override. One with `left_bar = true` reads back as no opinion, which every set but Clean resolves to shown, so no existing window changes.

**Clean the design** (`app/themes/clean.toml`, `name = "clean"`, `skin = "clean"`, `ui = "clean"`) holds the tube and the light off while it is worn. The resolver flattens the tube, and a fourth `TerminalLightPreset::Clean` draws 0% letter glow focused and unfocused. The resolved copy's phosphor and linger read 0, and the stored GAUGES dials are untouched. Quiet retires through `read_design_id` (`quiet-command` → `clean`).

**Nearly square** is a skin metric, `corner_cap`, applied by every radius helper, plus the six hard-coded sites through a new `Skin::capped`. The window frame, screen, pane frame, header ×, chips and float strip get it. `app/skins/clean.toml` caps at 3 px, with 2 px controls and no glow. Existing skins have no cap, so they draw exactly as before.

**New windows.** `~/.config/terminal-delight/new-window.toml` holds an optional `[theme]` and `[ui]`. A fresh session takes them, else the house look (now Clean). "use this window" writes the file; "Clean" deletes it.

## Least-confident decisions

1. Overrides survive a design change. Flip the agent counter on under Clean, then pick Hacker, then Clean again, and it stays on. That reads as "Clean never blocks", but it could surprise.
2. The menu-glyph group is not a key of its own. The group switch writes six overrides; a group key would be a second source of truth for the same six things.
3. Folding rows is an OUTER switch (`fold_rows`), on in Clean and off in All, not a property of the palette.
4. `corner_cap` caps the window frame too, which no skin has touched before.

## Slices

1. Tracer: the Clean design file, Quiet retired, the light preset, the tube and light holds; new windows wear Clean.
2. The UI set layer: files, the resolver, `[ui]` persistence, the left-bar migration; the outer gates and CONFIG.
3. Pane header gates, the ⋯ menu order, the waiting state word.
4. UI CONFIG in the painter palette, with OUTER | PANES, the set row and NEW WINDOWS.
5. Nearly square: `corner_cap`, the clean skin, a single-border selection.
6. Folding rows: 900 ms open, a click opens at once, 400 ms fold.
7. Nine languages, the authoring guide for a UI set, tests, and the CI gates. Build, and open on workspace 7.
