# Gate 2 · Architecture (with Gate 3 folded in)

## The shape of it

Three axes, not two. Terminal Delight already separates **colour** (the palette) from
**texture** (the CRT effects) and hot-reloads each. A skin is the third: **shape**.

```
palette  ──┐
 (colour)  │
           ├──►  Skin::bake(&Theme, scale)  ──►  Skin  ──►  sk.panel() / sk.chip() / sk.rule_h()
skin ──────┘                                                 │
 (shape)                                                     └──► one gpui element, strategy already resolved
effects  ─────────────────────────────────────────────────►  the renderer (unchanged)
```

A `SkinSpec` is what a file says — every token optional, nothing resolved, no palette
in it. A `Skin` is that spec baked against one palette at one scale, which is what a
chrome region holds while it builds elements. One spec therefore serves every pane
whatever theme each pane is wearing, and baking costs about forty float operations —
done once per region per frame, never per element.

## Layer 1 · Tokens

Three families, all in `app/src/skin.rs`, all generated from a single list by
`ink_tokens!` / `metric_tokens!` so adding a token is one line rather than four edits
that can drift apart.

**Inks** — 25 semantic chrome colours, named by job rather than by lightness: `panel`,
`panel_raised`, `panel_sunken`, `rule`, `rule_strong`, `edge`, `ink`, `ink_dim`,
`ink_off`, `ink_faint`, `mark`, `mark_wash`, `row_active`, `hover`, `live`, `ok`,
`warn`, `danger`, and the rest. The set is not invented: every default recipe is a
transcription of an expression that was already in `main.rs`, which is the evidence
that the vocabulary is the right size. `ink_faint` is `text.alpha(0.45)` because 23
call sites spell exactly that.

**Metrics** — 14 geometry values in unscaled logical pixels. The window's UI scale is
applied once, by `Skin::px`, instead of at the 688 sites that spell `px(9. * s)`.
Border and rule *widths* are the deliberate exception and never scale: a hairline that
thickens with the scale stops being a hairline, and today's chrome already draws them
unscaled.

**Shapes** — five strategy enums, and these are the part that makes a *look* rather
than a tint, because they change which element gets built:

| strategy | values |
|---|---|
| `corner` | `round` · `square` |
| `boundary` | `hairline` · `double` · `inset` · `none` |
| `emphasis` | `fill` · `underline` · `bracket` · `rail` |
| `divider` | `line` · `double` · `none` |
| `caps` | `off` · `upper` · `tracked` |

## Layer 2 · A vocabulary, not a stylesheet

`panel` · `bar` · `row` · `rule_h` · `rule_v` · `chip` · `btn` · `icon_btn` · `field` ·
`label` · `brackets` · `active_row` · `caps`.

Every call site asks for an **element**. The strategy branch happens inside the helper,
once, and never at the call site — which is precisely what makes a second skin a file
rather than a patch. `sk.chip(active)` is a tinted pill under the default skin and four
corner ticks under deco, and `render_left_bar` does not know which.

Helpers are generic over the element (`E: Styled + ParentElement`) so they work on a
`Stateful<Div>` — anything the chrome gave an id to — as well as a bare `Div`. gpui
spells border widths as `border_0 … border_8` rather than taking a length, so a private
`width_router!` macro routes a token width to the right one.

## Layer 3 · Files

`app/skins/*.toml`, embedded as builtins and hot-reloaded from `$TD_SKIN` or
`~/.config/terminal-delight/skin.toml`, mirroring `theme::init` exactly — a 300ms mtime
poll, and a parse error keeps the skin already loaded rather than flashing the default
through every save.

Resolution order: the user's own file, then the builtin the active theme asked for
(`skin = "deco"` at the top of a theme file), then `default`. A theme naming a skin is
how one click moves both axes without a second picker; the user's own file still wins,
because that is the edit loop they are standing in.

## The four decisions worth arguing with

**1 · A skin declares recipes, not colours.** `rule = { from = "surface", l = 0.30 }`
rather than `rule = "#1a2226"`. The first is still right under a palette the skin
author never saw. Pinning is still available (`mark = "#c8a44d"` for a look that must
be gold whatever the palette), and the two are different variants in the type because
they are different claims. Five palettes times two skins is ten working looks, not ten
files — and that, not the token list, is what makes the layer worth its weight.

**2 · Absent is not zero.** Every field a file can carry is an `Option`. A skin that
says only `corner = "square"` is complete and valid: every ink it did not mention
resolves to its *default recipe* over the live palette. The `None` arm reaches for the
recipe, not for a default value, so a missing token is never black, never `0.0`, and
never silently the same as a declared one.

**3 · The default skin must reproduce today's chrome, and a test says so.**
`default_skin_reproduces_todays_chrome` asserts the resolved default equals the
literals `main.rs` draws with — `rule_strong == darken(surface, 0.3)`,
`mark_wash == accent.alpha(0.14)`, `radius() == px(4)`, `radius_lg() == px(10)`. This
is the property that makes converting a call site safe: if adopting a token changes the
default look, the build fails before anyone has to notice by eye. (Verified failable:
moving `mark_wash` from 0.14 to 0.20 fails it.)

**4 · A typo is reported, not dropped.** Unknown keys are collected into
`SkinSpec::unknown` and printed with the nearest known key —
`ink.rule_stong (no such token — did you mean rule_strong?)`. A silently ignored token
is how theming systems produce a file that looks right, parses fine, and does nothing.

## What was rejected

- **Folding the tokens into `Theme`.** One file, one reload path, one tray — but it
  welds shape to colour, and the existing `$TD_PALETTE` split is the codebase's own
  argument that those axes come apart. A deco shape over the hacker palette has to be
  a thing you can have.
- **Recipes that can reference other inks.** It would let a skin say "the rule is my
  panel, lifted" — and it turns resolution into a graph walk with cycles to detect.
  Roles only.
- **Chamfered corners** (`Corner::Chamfer`), which is the deco move a reader expects.
  gpui cannot cut a corner off a div; it needs a path, and a path per chrome element is
  a rendering change, not a token. `Square` plus `brackets` gets most of the read for
  none of the risk. Worth revisiting if the look lands.
- **Letter-spacing as a style property.** gpui has none, so `Caps::Tracked` inserts
  U+2009 THIN SPACE between letters — in the string, behind `Skin::caps`, so exactly
  one place does it and a test can check it without a window.

## How it gets inspected

`terminal-delight skin --skin deco --theme deco` resolves headlessly and prints every
token as JSON. A skin is data, and data nobody can read back is data nobody can debug.

It is also the only sanctioned source of token values for a mockup or a document:
anything that recomputes the recipes outside this binary is a second implementation
waiting to disagree with the first.

It earned itself on first run. Two defects were visible in its output and neither was
visible in the code:

- the deco palette's `ansi3` was the same brass as its accent, so `warn` resolved to
  exactly `mark` — a pane at 90% of its ceiling would have been painted in the colour
  of the frame around it;
- `ok` defaulted to `ansi2`, and `field-command` — a theme we already ship — points its
  accent at its own `ansi2`, so the same collision existed on a second pairing.

The second one says the *recipe* was wrong, not the palette: a terminal palette very
often points its accent at one of its own normal colours. The state inks now default to
the bright slots, and
`no_state_ink_collapses_onto_the_accent_in_any_builtin_pairing` checks every skin
against every theme rather than the one pairing a person happened to look at.
