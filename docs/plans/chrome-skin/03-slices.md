# Gate 4 · Vertical slices

Adoption is staged, one surface at a time, because the default skin reproduces today's
chrome exactly — so a half-converted app is not a broken app, it is an app where some
surfaces can be restyled and the rest cannot yet.

Sizes are measured, not guessed: the count is `px(` + `rounded_*(` + `.border_*(` +
`.alpha(` + `darken(` + `brighten(` + `hsla(` occurrences in the surface's line range
on `main.rs` at ed69a0f. Not every one becomes a token — layout padding stays layout —
but the number is the honest upper bound on the work.

| # | Slice | Literals | State |
|---|---|---|---|
| 0 | **Tracer: the left bar's frame, header and active row** | — | **built** |
| 1 | **The rest of the left bar: branch rows, task rows, the usage slot** | 194 | **built** |
| 2 | **The per-pane header and frame (`pane.rs`)** | 223 | **built, shape only** |
| 3 | **The mother bar and tab strip** | 117 | **built** |
| 4 | Overlays: usage, savings, find, plugins, the pickers | 180 | costed |
| 5 | The theme tray and menus | 315 | costed |
| 6 | Workspace body, drag affordances, the rest | 359 | **shape only** |

## The MVP cut, and why it is not slices 1–3 in full

Reviewed 2026-09-12 with *"MVP and look before getting too deep"*. So what landed is
not three whole slices — it is the **shape-carrying half of all of them**, which is a
different and much better-shaped piece of work.

The insight is that a surface has two kinds of literal in it, and only one of them
decides whether a look lands:

- **Shape** — 99 radius calls, the boundary colours, the bezel's glint and seat, the
  emphasis on a selected thing. Change these and the app is a different app.
- **Layout** — padding, gaps, row heights, one-off tints. Change these and almost
  nothing reads differently; leaving them as literals costs nothing today.

Converting the shape half of six surfaces beats converting all of two, because the
question being answered is *"does deco look right"* and only the shape half answers
it. The layout half stays literal until a slice has a reason to touch it.

What that means concretely: under the deco skin the bar, the strip, the left bar, the
pane headers and the workspace body are square, gold-edged, bracket-marked and flat.
The **overlays and the theme tray are not** — open the usage face or the paint tray
under deco and you get rounded corners on a square app. That is the visible seam, it
is deliberate, and it is what slices 4 and 5 close.

## What the bezel button cost, and why it moved

`bezel_btn_s` is the chrome's main button — split, new-tab, the left bar's adopt and
`+`, and twenty-five more sites. It carried a white inset glint and a drop shadow: a
raised physical key. Under a skin that has declared `shine = "flat"` those are not a
style preference to turn off, they are a contradiction — the glint asserts the button
is above the surface while the hard edge asserts the surface is a plane.

So the button moved into the vocabulary as `Skin::bezel`, and `bezel_btn_s` became a
two-line delegate. Its signature changed from `&Theme` to `&Skin` and every call site
with it — which is the load-bearing detail: the helper stopped needing the palette at
all, because the skin already carries every colour it used. A helper that can drop its
`&Theme` parameter entirely is the clearest evidence available that the token set is
the right one.

## Slice 0 — the tracer, built

Converted in `render_left_bar` and `task_row`:

- the bar's outer frame → `sk.panel()`, which carries ground, corner and boundary;
- its header row → `sk.row()`, and the divider under it → `sk.rule_h()`;
- the scope chip → `sk.chip(scoped)` + `sk.caps()`;
- the fold and hide buttons → `sk.icon_btn(id)`;
- the active task row's emphasis → `sk.active_row(d, is_active)`.

Chosen for three reasons. It is the surface Parker's reference screenshots actually
are — a left nav of rows with status chips is the HumanLayer shape. It carries one of
every device the vocabulary has (a framed region, a banded header, a rule, a chip, an
icon button, an active row), so the design is tested rather than asserted. And it is
contiguous, which matters when other agents are editing `main.rs` on the same branch.

**What changed in the default look:** one thing, deliberately. Two triangle glyphs in
the bar header were drawn at `text.alpha(0.75)` and `text.alpha(0.70)`; both now read
`ink_dim`, which is `0.70`. Preserving a 5% alpha difference between two adjacent
glyphs would have been preserving the disease — 59 distinct alphas over six roles is
the thing this layer exists to end. Everything else is value-identical, and the golden
test is what says so.

## The order, and why

**Slice 2 (the per-pane header) before slice 3 (the mother bar)**, even though the
mother bar is more visible. The pane header repeats once per pane — fourteen times on a
working screen — so it is where a shape change reads hardest, and `pane.rs` is a
quieter file than `main.rs`, which four `chrome/*` branches merged into on 2026-09-12
alone. Convert where the conflict cost is lowest while the vocabulary is still moving.

**Slice 5 (the theme tray) late, despite being where a skin picker would go.** The tray
is the most idiosyncratic surface in the app — colour disks, swatch grids, a paint
shelf — and it is the one most likely to need a token invented for it alone. Doing it
early would push a one-surface token into the shared vocabulary.

## The thing to watch across slices

Each slice reports how many **new tokens** it had to add. That number is the honest
post-hoc check on the 8/10 in `00-status.md`: a design that was specified well enough
needs none, and a design that was guessed needs one per surface.

**The MVP cut added two: `edge_strong` and `ink_off`, plus one strategy, `shine`.**

That is inside the "two or three" the plan set as its own failure line, and the shape
of what was added is more informative than the count. Both inks came from sites that
already existed six and one times over (`accent.alpha(0.50)` on a pane's own chrome,
`faint` as a not-in-effect label) — so they were vocabulary that was already being
spoken, not tokens invented for one surface. `shine` is a genuine gap: the original
five strategies had nothing to say about whether a surface is lit, and a look cannot be
"modern and professional" while every button keeps a white glint on its top-left edge.

**A correction to the plan, made during the cut.** The plan said the per-pane header
should come before the mother bar because `pane.rs` is quieter. That ordering was right
for conflicts and wrong for a look: the strip is where a person decides whether a skin
works, so it went in the same pass. The conflict risk was real and was simply not
taken — `origin/main` had not moved.

## What would say this was the wrong layer

- Slices 1–3 add more than two or three tokens between them. The vocabulary was too
  small, and a token invented per surface is a constant with extra steps.
- A slice needs a call site to branch on which skin is active. That is the one thing
  the vocabulary exists to prevent; if it happens, the strategy enum is missing a
  value, and the fix is in `skin.rs`, never at the call site.
- Nobody writes a third skin. Two skins is a refactor that paid for itself in
  tidiness; the layer was justified on the claim that the third one is cheap.
