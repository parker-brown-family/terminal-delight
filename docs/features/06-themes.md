# Themes — per-pane, hot-reloaded, data-driven

Every pane can wear its own look, live-editable from a data file with no recompile.
Themes are how the wall stays readable at a glance (program/project colour) *and*
beautiful (phosphor, amber, tactical cyan, light "quiet command").

## Why it matters

"Modify on the fly" is a day-one principle: change a colour or an effect in a TOML
file and the running app picks it up in ~300 ms. And because themes are **per-pane**,
one terminal can be hacker-green while the next is tactical-cyan — no global flip.

## Features

| Feature | What it does | Evidence | Control |
|---|---|---|---|
| **Hot-reload** | Edit `~/.config/terminal-delight/theme.toml`, ~300 ms mtime poll swaps it live | theme watcher | `TD_THEME` path |
| **Built-in pack** | hacker (phosphor) · tactical-overdrive (cyan) · field-command (olive) · quiet-command (light, effects off) · gamba (satire) | `BUILTIN_THEMES` | picker |
| **Colour-set presets** | bat · cherry · clowndy · wood · army · midnight · snowflake — 7 tray presets w/ per-set swatch glyph | dynamic palette + `swatch()` | tray |
| **Seed colour wheel** | Pick an accent on a canvas HSV wheel → harmonic palette auto-generated; 3-marker | wheel + `hsla_to_hex` | breakout picker |
| **Colour modes** | Default (real ANSI) · Monochrome (phosphor ramp) · OnTheme (harmonic around seed) | `ColorMode` enum | cycle |
| **Syntax overlay** | 4 schemes (code/agentic/logs/markdown) on a 6-role palette; orthogonal to colour mode | `SyntaxScheme` | SYNTAX tray |
| **Human-input colour** | Your own turns in an agent TUI get a distinct colour (whole wrapped message) | `Theme::human`, `human_input_rows` | 👤 pip |
| **Per-pane theme + grade independence** | Theme group and grade group inherit from the outer **independently**; live "follow outer" toggle, non-destructive | `PaneTheme` | per-pane toggles |
| **Monitor-OSD grade** | 6 sliders — brightness/contrast/colour/text/background/gamma — applied as HSLA at paint time | `Grade`, `GradeKey` | ⛭ DISPLAY tray |
| **Text-size channel** | One dial scales terminal text *and* chrome together (reflows) | `GradeKey::TextSize` | scrubber / Ctrl+scroll |
| **Warp / crawl channels** | Per-pane curvature + crawl angle/depth as grade channels | `GradeKey::{Warp,Crawl*}` | DISPLAY tray |
| **TOML format** | Human-readable: `[colors]` hex, `[effects]` 0..1 floats, `[font]`, optional icon glyph; lenient parse | `ThemeFile` | edit by hand |
| **Picker UI** | Browse built-ins + custom, glyph icon, hover tooltip, "open in editor" for custom | theme picker | right-click |

## The model

Appearance = **Texture × Colour-set × global Warp toggle**, with a 3-marker HSL
wheel and a two-axis text-colour stack (SOURCE × SYNTAX). The MCP `set_pane_config`
can remote-drive any grade channel (appearance only — never a PTY).

## Status

**Shipped.** The default house config is a warm amber outer cabinet + green inner
terminals at a tasteful (non-blown-out) grade.
