# The look

What a brief should feel like, so you can build toward it rather than copy it. `base.css`
already styles every component the briefs use; this page is for the part it cannot do for
you, which is making the thing you draw belong on the page.

Parker, 2026-09-25: *"WE WANT OUR BRIEFS to lok GOOOOOOOOD and DRIP with style!"* The look
he pointed at was Terminal Delight's docs redesign, and the reference build is
`~/Work/reports/2026-09-25-briefs-in-glass.html`.

## What we are going for

A brief is a pane of Terminal Delight. Behind it is a wallpaper you only glimpse, blurred
and dimmed by the theme's own ground. Everything a reader touches is glass laid over that:
lit along its top edge, a darker lip at the bottom, the wall faintly visible through it.
Anything that is a machine (a command, a log, a terminal) is opaque black, because a
screen you can see the wall through is not a screen. One element per page, the thing the
reader must not miss, is milky glass: bright, lit from above, dark ink (`.glass`).

The theme is picked at random when the brief is built, one of 27 Omarchy themes and one
of its wallpapers, and nobody switches it afterwards. There is no theme switcher and no
pinning a theme to the subject; the variety is the point. `brief-wall --theme` and
`--image` exist so a rebuild can keep the wallpaper it already has.

## What makes it work

- **Depth comes from layers, not borders.** A panel is lighter glass on darker glass,
  with a highlight on top and a shadow beneath. A plain 1px box on a flat colour is what
  the old stylesheet did.
- **The theme owns the ground, the ink and the accent; meaning owns everything else.**
  Draw with `var(--acc)`, `var(--fg)`, `var(--dim)` and the svg classes, so a figure
  changes with the wallpaper. Red is still a decision waiting and green still the
  recommended path, in every theme (`reference/pictures.md`).
- **Numbers glow.** A figure worth reading as a number is set large, in the accent or a
  status colour, with a soft glow of its own colour. It should read from across the room.
- **Slants and segments read as instruments.** A skewed bar, a chevron, a stack of slanted
  slabs look measured and alive where a rectangle looks like a form. The two examples
  below are both built from that.
- **Texture goes under the text, never over it.** Scanlines, a sheen, a grain: behind or
  inside a surface, never across the words.
- **Restraint is what makes the drip visible.** One milky card, one glowing number per
  screen, one lit option per group. A page where everything glows reads as nothing
  glowing.

Beyond that, find your own way. Build what the material needs. These two are here
because Parker singled them out, not because every brief needs them.

## Example 1: chopped bars

A stack of slanted slabs, one per layer, read bottom to top, each labelled on the right
through a dotted leader. It suits anything layered or ordered: a stack, a pipeline
seen from the side, what sits on what. Parker, on the six-layer figure in the reference
build: *"chopped bars are super nice to look at."*

Each slab is one parallelogram, `x+s,y  x+w+s,y  x+w,y+h  x,y+h`, stepped down by the
same amount. Colour one with a meaning class to single it out.

```html
<figure>
  <span class="lbl">01 &middot; What sits on what</span>
  <div class="scroller">
    <svg viewBox="0 0 960 280" role="img" aria-label="Four stacked slabs, one per layer">
      <polygon points="100,210 460,210 410,254 50,254" class="box"/>
      <polygon points="100,152 460,152 410,196 50,196" class="box ready"/>
      <polygon points="100,94 460,94 410,138 50,138" class="box"/>
      <polygon points="100,36 460,36 410,80 50,80" class="box mine"/>
      <g class="wire" stroke-dasharray="2 4">
        <line x1="464" y1="58" x2="530" y2="58"/><line x1="464" y1="116" x2="530" y2="116"/>
        <line x1="464" y1="174" x2="530" y2="174"/><line x1="464" y1="232" x2="530" y2="232"/>
      </g>
      <g font-family="ui-monospace, monospace">
        <text x="542" y="54" class="ink" font-size="14" font-weight="700">proposed</text>
        <text x="542" y="72" class="ink2" font-size="12">the layer this brief adds</text>
        <!-- one pair per slab, 58 apart -->
      </g>
    </svg>
  </div>
  <figcaption>A stack read bottom to top. <b>The amber slab is the one being proposed</b>.</figcaption>
</figure>
```

The reference build goes further: its six slabs each draw what their layer is (a gradient
for the wallpaper, a blurred copy for the blur, a line pattern for the scanlines). Let a
slab look like its layer when it can.

## Example 2: a readout, two numbers in one glance

From the Target Race calculator on brownfamilysports.com
(`brown-family-sports-web/running-pace-calculators/index.html`, `.tr-holo`). A chevron
plate carries two variables at once: the fill runs to the likelihood, and the pips light
up with how much evidence stands behind it. Parker: *"really nifty to show a
multi-variable analysis where you can see the pips in evidence go up, but so does the
background fill."* It stops a reader trusting a full bar with one pip lit.

The plate is an SVG stretched behind ordinary HTML (`preserveAspectRatio="none"`, with
`non-scaling-stroke` so the rim stays thin). The fill's `width` is the probability times
640. Put the readout inside a `figure` so it takes a note.

```html
<div class="readout">
  <svg class="readout-plate" viewBox="0 0 640 300" preserveAspectRatio="none" aria-hidden="true">
    <defs>
      <filter id="ro-glow" x="-30%" y="-30%" width="160%" height="160%"><feGaussianBlur stdDeviation="4"/></filter>
      <clipPath id="ro-clip"><polygon points="8,10 590,10 632,150 590,290 8,290 42,150"/></clipPath>
      <pattern id="ro-scan" width="4" height="4" patternUnits="userSpaceOnUse"><rect width="4" height="1" fill="#fff" opacity=".07"/></pattern>
    </defs>
    <polygon class="glow" points="8,10 590,10 632,150 590,290 8,290 42,150" filter="url(#ro-glow)"/>
    <polygon class="plate" points="8,10 590,10 632,150 590,290 8,290 42,150"/>
    <g clip-path="url(#ro-clip)">
      <rect class="fill" x="0" y="0" width="416" height="300"/>   <!-- 0.65 × 640 -->
      <rect class="edge" x="413" y="0" width="3" height="300"/>
      <rect width="640" height="300" fill="url(#ro-scan)"/>
    </g>
    <polygon class="rim-dark" points="8,10 590,10 632,150 590,290 8,290 42,150"/>
    <polygon class="rim" points="8,10 590,10 632,150 590,290 8,290 42,150"/>
  </svg>
  <div class="readout-body">
    <p class="k">Likelihood the split lands this week</p>
    <div class="big">65%</div>
    <p class="verdict">Likely. Two of the three blockers are already merged.</p>
    <p class="k">Evidence</p>
    <div class="pips" role="img" aria-label="evidence 3 of 5"><i class="on"></i><i class="on"></i><i class="on"></i><i></i><i></i></div>
    <p class="why">Read from the test run, not a benchmark: fair support.</p>
  </div>
</div>
```

```css
.readout { position: relative; max-width: 640px; }
.readout-plate { position: absolute; inset: 0; width: 100%; height: 100%; overflow: visible; }  /* or the glow is cut off at the box */
.readout-plate :is(polygon, rect) { vector-effect: non-scaling-stroke; }
.readout .glow { fill: none; stroke: var(--acc); stroke-width: 13; opacity: .45; }
.readout .plate { fill: color-mix(in srgb, var(--bg2) 72%, transparent); }
.readout .fill { fill: var(--s3); opacity: .4; }   /* the band: --s3 likely, --warning maybe, --critical unlikely */
.readout .edge { fill: #fff; opacity: .85; }
.readout .rim-dark { fill: none; stroke: var(--bg0); stroke-width: 6; stroke-linejoin: round; }
.readout .rim { fill: none; stroke: color-mix(in srgb, var(--acc) 60%, #fff); stroke-width: 1.7; stroke-linejoin: round; }
.readout-body { position: relative; padding: 24px 70px 22px 50px; }
.readout .k { margin: 0 0 10px; font: 800 10.5px/1.3 var(--mono); letter-spacing: .2em; text-transform: uppercase;
  color: color-mix(in srgb, var(--acc) 55%, #fff); text-shadow: 0 0 9px color-mix(in srgb, var(--acc) 80%, transparent); }
.readout .big { margin: 0 0 10px; font: 400 76px/1 Georgia, "Times New Roman", serif; color: var(--yel);
  -webkit-text-stroke: 1.5px var(--bg0); text-shadow: 0 0 14px color-mix(in srgb, var(--yel) 60%, transparent); }
.readout .verdict { margin: 0 0 4px; color: var(--fgb); font-weight: 650; }
.readout .why { margin: 12px 0 0; font-size: 14px; }
.pips { display: flex; gap: 8px; max-width: 340px; }
.pips i { flex: 1; height: 16px; transform: skewX(-15deg); border: 1.5px solid var(--bg0); border-radius: 2px;
  background: color-mix(in srgb, var(--acc) 7%, transparent); }
.pips i.on { background: linear-gradient(180deg, color-mix(in srgb, var(--acc) 40%, #fff), var(--acc));
  box-shadow: 0 0 11px color-mix(in srgb, var(--acc) 85%, transparent), inset 0 0 3px #fff, 0 0 0 1.5px var(--bg0); }
.pips.thin i.on { background: linear-gradient(180deg, #ffdf9c, var(--warning)); box-shadow: 0 0 11px #fab219cc, 0 0 0 1.5px var(--bg0); }
```

On the calculator the pips turn amber (`.pips.thin`) when the evidence is too thin to
lean on, and the sentence under them changes from a verdict into a request for better
evidence. Carry that over: a readout that cannot be trusted should say what would fix it.

Two things the snippet does not do for you. The plate's ids (`ro-glow`, `ro-clip`,
`ro-scan`) must be unique on the page, so a second readout needs its own. And the plate
is `aria-hidden` because it is decoration: the pips carry the label, and the words say
the rest.

## The concur stamp

`notes.js` draws it, and it is not yours to restyle, but it is part of the look: a wet
rubber stamp with air pockets in the ink that lands with a thud and jolts the decision
under it. The thud and the jolt only play in a browser. Terminal Delight's document square
draws a screenshot of the page, so it shows the stamp already landed; the ink's pockets are
sized to survive that.
