# Terminal Delight — Brand Pack

> The terminal you watch your fleet through. CRT-retro meets agent-native.

Master logo: [`assets/terminal-delight-logo-full.png`](assets/terminal-delight-logo-full.png)
· Visual sheet: [`brand-sheet.html`](brand-sheet.html)

The icon **is the product**: a curved CRT screen showing a four-quadrant agent
wall — green / amber / cyan / violet panes glowing behind glass. The wordmark
flows green→cyan; the tagline carries the violet.

---

## Palette

Sampled directly from the master logo (exact hexes, not eyeballed).

| Role | Name | Hex | RGB | Use |
|------|------|-----|-----|-----|
| Canvas | **Void** | `#030708` | 3,7,8 | Default background. Brand is dark-first. |
| Primary | **Signal Green** | `#67F454` | 103,244,84 | The core brand colour. Wordmark start, primary glow, "live" agent. |
| Primary (deep) | **Phosphor Green** | `#14DC4E` | 20,220,78 | Saturated green for fills/bars where bright green blooms too much. |
| Secondary | **Cyan** | `#29E3ED` | 41,227,237 | Wordmark end, secondary accent, the cyan pane. |
| Accent (warm) | **Amber** | `#E5AA0D` | 229,170,13 | The warm quadrant; caution/attention; balances the cool palette. |
| Accent (cool) | **Violet** | `#815BAD` | 129,91,173 | Tagline + quadrant divider; "agentic" / AI accent. |
| Text on dark | **Phosphor Grey** | `#B5BEAC` | 181,190,172 | Body copy, captions, UI labels on Void. |
| Glass tint — green | — | `#1C4E2C` | 28,78,44 | Recessed-glass / muted surface tints. Not for type. |
| Glass tint — violet | — | `#332D42` | 51,45,66 | " |
| Glass tint — amber | — | `#4F5028` | 79,80,40 | " |

### Extended / UI palette (sampled from the brand-pack poster)

The vivid set above is for **logo + hero**. The brand-pack poster defines a
calmer, desaturated set for **UI surfaces and chrome** plus a neutral ramp — use
these in-app/in-product so the bright brand colours stay reserved for accents.

| Role | Hex | Use |
|------|-----|-----|
| **Deep Navy** | `#020C16` | UI canvas (cooler than Void; section backgrounds) |
| **Surface** | `#0C1823` | cards / panels |
| **Surface raised** | `#14262C` | inset / active panels |
| **Panel line** | `#282E32` | borders / dividers |
| Neutral 600 | `#5A6061` | muted text / icons |
| Neutral 400 | `#999DA0` | secondary text |
| Neutral 100 | `#DFE2E3` | high-contrast labels |
| Green (muted) | `#44B064` | UI "live" / success (calmer than Signal Green) |
| Teal (muted) | `#45A4AE` | UI secondary |
| Amber (muted) | `#C89944` | UI warning / attention |
| Violet (muted) | `#916D86` | UI agentic accent |

**Signature gradient** (the wordmark, and any hero treatment):
`linear-gradient(90deg, #67F454 0%, #29E3ED 100%)` — green left → cyan right.
Optionally extend to violet for a full-spectrum sweep:
`#67F454 → #29E3ED → #815BAD`.

**Quadrant mapping** (the icon's meaning — keep it consistent everywhere the
agent wall is depicted): top-left **green**, top-right **amber**, bottom-right
**cyan**, bottom-left **green**, divider glow **cyan→violet**.

### Contrast / accessibility
- Green `#67F454`, Cyan `#29E3ED`, Amber `#E5AA0D` and Phosphor Grey all clear
  AA for large text on Void `#030708`. Violet `#815BAD` on Void is **decorative
  only** (fails AA for body) — use it for the tagline, rules, and glow, never for
  running text.
- Never put Signal Green text on Cyan (or vice-versa) — they vibrate. Separate
  with Void or a glass tint.

---

## Typography

Two faces, per the brand-pack poster: a **geometric display sans** for the brand
name + headings, and a **monospace** for terminal/code/tagline contexts.

| Slot | Typeface | Fallback stack | Treatment |
|------|----------|----------------|-----------|
| **Display / Wordmark** | **Space Grotesk** | `"Space Grotesk", Inter, system-ui, sans-serif` | the brand name; lowercase hero lockup, green→cyan gradient on Void |
| **Tagline** | mono | `"JetBrains Mono","Space Mono",ui-monospace,monospace` | UPPERCASE, `letter-spacing: 0.35em`, violet `#815BAD`, flanked by ⟩ … ⟨ rule marks |
| **Headings** | Space Grotesk | sans stack above | Phosphor Grey or Signal Green |
| **Code / terminal / data** | mono | mono stack above | the in-product voice; Phosphor Grey on a dark surface |
| **Body** | Space Grotesk or mono | either stack | Phosphor Grey `#B5BEAC` / Neutral 100 `#DFE2E3` |

Space Grotesk carries the **brand** (marketing, site, headings); the **mono**
carries the **product** (the terminal itself, code, the tagline). Keep the hero
wordmark lowercase; never title-case or all-caps "Terminal Delight".

---

## Tagline & voice

**Primary tagline:** `AGENTIC FORWARD`
Alt one-liners (for hero/social/README): *"The terminal you watch your fleet
through." · "A CRT for the age of agents." · "Watch your agents work."*

**Voice:** confident, practitioner-to-practitioner, a little retro-futurist.
We are the **HUD over your agent fleet** — observability and craft, not an
orchestrator. Avoid hype-speak; show the glass and the glow, let it speak.

---

## Logo usage

- **Clearspace:** keep a margin of at least the icon's corner-radius (≈ one
  "screen bezel" width) on all sides; no other element intrudes.
- **Min size:** icon ≥ 32 px; full lockup (icon + wordmark) ≥ 240 px wide. Below
  that, use the icon alone.
- **Backgrounds:** always on Void or true black. On unavoidable light contexts,
  use a dark plate behind the mark — do not place the glow mark on white.
- **Don't:** recolour the wordmark to a flat single colour (keep the gradient);
  stretch/rotate; add a drop shadow beyond the built-in glow; reorder the quadrant
  colours; title-case the wordmark; place green type on cyan.

---

## Asset inventory

| File | What | Status |
|------|------|--------|
| `assets/terminal-delight-logo-full.png` | Master full lockup (1284×1012) | ✅ in repo |
| `assets/terminal-delight-brand-pack-reference.png` | Brand-pack reference poster (1024×1536) — logo suite, components, mockups | ✅ in repo |
| `brand-sheet.html` | Visual brand sheet (logo + palette + type) | ✅ in repo |
| `assets/terminal-delight-icon.png` | icon-only, square 640² (cut from master, verified) | ✅ in repo (black-bg; transparent TODO) |
| `assets/terminal-delight-wordmark.png` | wordmark-only 1080×150 (cut + verified) | ✅ in repo (black-bg; transparent TODO) |
| `assets/favicon-{16,32,180,256,512}.png` · `apple-touch-icon.png` · `favicon.ico` | favicon / app-icon set | ✅ in repo |
| stacked lockup (icon over wordmark) | square/vertical placements | ▢ TODO |
| **TD** monogram | tight square mark / avatar | ▢ TODO |
| badge variants (per the poster) | UI chips / stamps | ▢ TODO |
| mono / 1-colour mark | stamps, embroidery, small print | ▢ TODO |
| social card (1200×630 OG) | links/share | ▢ TODO |
| SVG icon | scalable / crisp | ▢ TODO (raster master → trace) |
| transparent-bg icon + wordmark | overlay on non-black | ▢ TODO (glow makes clean alpha tricky) |

The **reference poster** (`assets/terminal-delight-brand-pack-reference.png`) is
the visual source of truth for the logo suite, iconography, component patterns,
and mockups — this `BRAND.md` is the machine-readable distillation of it.

**Next:** cut the icon-only / stacked / **TD** monogram + wordmark crops from the
masters, generate the favicon set + an OG social card, then wire the favicon into
the site and the mark into `README.md`.
