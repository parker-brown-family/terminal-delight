# Gate 1 · Product

## What was asked for

An art-deco look for Terminal Delight — BioShock's instrument panels crossed with
HumanLayer's dark agent dashboard — modern and professional, so *"the HUD noise and
flourishes are out, just border styling and bars and boundaries and other attentional
strategies are in"*. And, named as the major part of the job: making the visual surface
**easily and repeatably themeable**, with the note that it would take careful
architectural work.

The second half is the product. The art-deco skin is how we find out whether the first
half is true.

## What is actually in the way

Terminal Delight is already themeable on two axes, and hot-reloads both:

- **colour** — a `[colors]` palette, separable as `$TD_PALETTE` so an external theme
  switch can retint the running terminal;
- **texture** — `[effects]`, how much CRT you get.

Neither reaches the chrome. Measured on `main` at ed69a0f:

| | count | what it means |
|---|---|---|
| ad-hoc colour derivations in the chrome | **268** | `darken(th.surface, 0.3)`, `th.text.alpha(0.45)` — a colour computed at the call site cannot be themed |
| distinct alpha values over six palette roles | **59** | there is no shared vocabulary; each site invented one |
| radius calls | **141** | `rounded_sm` / `rounded_full` / `rounded_md`, all compiled in |
| border calls | **197** | |
| distinct `px()` literals | **62** | every spacing decision is a constant |

So a theme file can **retint** Terminal Delight and cannot **restyle** it. Art deco is
not a retint: it is square corners where there were round ones, a twin rule where there
was a hairline, a bracket where there was a filled pill, tracked caps where there was
sentence case. None of that is reachable from a colour.

## Who it is for, and what "repeatably" has to mean

Two audiences, and the second is the one that sets the bar.

1. **Parker**, who wants Terminal Delight to look like a deliberate instrument and
   wants that look to be changeable without a build.
2. **Whoever writes the third skin** — including an Omarchy theme switch, which already
   drives the palette axis and has no idea what our chrome is made of.

The test for (2) is not "a skin file exists". It is: *does a skin someone wrote against
the brass palette still look right on the green one?* If every skin has to be
hand-authored per palette, five palettes times three skins is fifteen files and the
system has failed even though it technically works.

That is what forces skins to be written as **recipes over palette roles** rather than
as colours. It is the single product requirement that shaped the architecture.

## Explicitly not in scope

- Restyling the whole chrome in one pass. Adoption is staged — see Gate 4 — and the
  shipped default skin reproduces today's look exactly so that staging is safe.
- A skin picker in the theme tray. A theme may *name* a skin (`skin = "deco"`), and
  `$TD_SKIN` / `~/.config/terminal-delight/skin.toml` override it. A second picker is a
  UI decision, and it can wait until there are more than two skins to pick from.
- Per-pane skins. The palette is per-pane; shape is not, and a window whose panes had
  different corner radii would read as broken rather than as configured.

## Done when

- A skin file changes Terminal Delight's chrome shape with no recompile.
- The shipped default skin reproduces today's chrome, asserted by a test rather than by
  eye.
- The deco skin renders the converted surface with square corners, a twin rule and
  bracketed emphasis, and no call site branches on which skin is active.
- The same deco skin is still correct on a palette its author never saw.
