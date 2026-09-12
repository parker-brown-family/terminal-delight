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
| 1 | The rest of the left bar: branch rows, the usage slot | 194 | costed |
| 2 | The per-pane header and frame (`pane.rs`) | 223 | costed |
| 3 | The mother bar and tab strip | 117 | costed |
| 4 | Overlays: usage, savings, find, plugins, the pickers | 180 | costed |
| 5 | The theme tray and menus | 315 | costed |
| 6 | Workspace body, drag affordances, the rest | 359 | costed |

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
needs none, and a design that was guessed needs one per surface. Record it in the slice
commit, not in a chat message.

## What would say this was the wrong layer

- Slices 1–3 add more than two or three tokens between them. The vocabulary was too
  small, and a token invented per surface is a constant with extra steps.
- A slice needs a call site to branch on which skin is active. That is the one thing
  the vocabulary exists to prevent; if it happens, the strategy enum is missing a
  value, and the fix is in `skin.rs`, never at the call site.
- Nobody writes a third skin. Two skins is a refactor that paid for itself in
  tidiness; the layer was justified on the claim that the third one is cheap.
