# Pictures

A brief is drawn, and the prose ends up as captions on the drawings. Read this before writing
the body, because the pictures decide what the body has to say.

## The standard

```
/home/parker/Work/reports/2026-09-15-attention-spine-groundwork.html
```

Eleven figures for a plan that two earlier briefs had already covered in prose. Parker on
seeing it: *"THAT IS AWARD WINNING WORK!!! ... all briefs we produce should have this much
visual richness and pictures ... super helpful for reading."* The two prose versions of the
same material got no such reaction. Copy its moves; the file is the reference implementation.

Everything in it is hand-built HTML, CSS and inline SVG. No chart library, no mermaid, no
image files, no external anything — a brief has to survive being emailed as one file, and
every picture in it survives that.

## Every picture is a `<figure>`

`base.css` styles `figure`, `.lbl`, `figcaption`, `.scroller` and `.gutter`: a pane of dark
glass, near-opaque so a drawing made for a dark page still reads over any wallpaper, with a
lamp beside the numbered label. **Do not restyle them in the brief's own block.** A brief's CSS
beats `base.css` by design (it sits in `@layer brief`), so a pasted `figure { background:
var(--surface-1) }` from an older brief silently replaces the glass with a flat box.

```html
<figure>
  <span class="lbl">03 &middot; What opening costs the wall</span>
  <div class="scroller"> …the picture… </div>
  <figcaption>Their geometry test, drawn. <b>Pinning is the only path that costs columns</b> —
  thirteen off each pane in your two-column tab, which is why it is opt-in.</figcaption>
</figure>
```

Four things that make that markup work:

- **Number the label.** `NN · short title`. `titleOf()` in `notes.js` reads `.lbl`, so the
  exported anchor comes out as `fig-03-what-opening-costs` — short, stable when you reword,
  and legible in the map.
- **The caption asserts something.** Not a description of what the reader can already see.
  Bold the load-bearing clause; that clause is what he will argue with.
- **`.scroller` around anything wider than the column.** The page must never scroll
  horizontally; a single wide diagram is the usual cause.
- **`figure` is in `notes.js` TARGETS**, so every picture takes a note with no extra work.
  The brief that set this standard had to override `NOTES_TARGETS` wholesale to get that.
  It is in the default list now — do not fork the list to add it again.

## Two kinds, and when each one is right

| Kind | Draw it when | Built from |
|---|---|---|
| **Mockup** | The reader has to judge a *surface* — what it will look like, at the size he will read it | HTML + CSS boxes, real proportions, real copy |
| **Diagram** | The reader has to judge a *mechanism* — flow, geometry, order, resolution, sequence | Inline `<svg viewBox>`, mono text, no library |

A mockup lies if it is pretty. Use the brief's own palette, the real strings, the real number
of rows, and let it look like the plain thing it will be. The at-size mockup — one picture
drawn at the true pixel width beside a column of prose — is the one that catches "that row
says the wrong thing" before code exists.

SVG rules that keep them legible: `viewBox` and no fixed `width`/`height`, `role="img"` with
an `aria-label`, `font-family="ui-monospace, monospace"` at 10–12px, and one `<marker>` in
`defs` for arrowheads.

**Colour a drawing with classes, so it wears the brief's theme.** Every brief now takes its
ground and ink from a random Omarchy theme (see *The look* in `SKILL.md`), so a box
hard-coded `#1a1a19` is a neutral grey slab on a tokyo-night blue or a quattrocento brown. A
presentation attribute (`fill="var(--bg2)"`) cannot read `var()`; a CSS class or a `style`
attribute can, and `base.css` ships a small set:

| Class | On a shape | On `text` |
|---|---|---|
| `.box` | panel fill, hairline stroke | — |
| `.wire` | a connector line, muted | — |
| `.ink`, `.ink2`, `.ink-acc` | — | bright ink, muted ink, the theme's accent |
| `.crit` `.fail` `.ready` `.struct` `.mine` `.unk` | the meaning's colour as stroke over a tinted fill; `.unk` is dashed | the meaning's colour |

Gradients take `style="stop-color: var(--acc)"`. Hex is still fine for the six meanings below,
because those colours do not change with the theme. The 2026-09-25 brief
`~/Work/reports/2026-09-25-briefs-in-glass.html` draws its figure 1 this way.

## The repertoire

Eleven pictures, drawn from the exemplar. Each earns its place under one condition.

| Picture | Earns its place when |
|---|---|
| **State triptych** — the same surface in each of its states, side by side | A thing has modes and the argument is about which mode is the steady state |
| **At-size mockup + prose beside it** | Width, density or row content is itself the decision |
| **Anatomy** — numbered pins on a mockup, keyed to a legend below | A unit of the design has parts, and the parts have different provenance |
| **Geometry before / after** | A change costs space, and the cost is the argument |
| **Pipeline** — stages left to right, amendments dropped in below where they apply | You are amending someone else's design and need to show it is not a redesign |
| **Decision tree** | An open question has a settled answer in the code, and one branch is the surprise |
| **Ordering** — the ranked lanes, with the source of the order named beside them | Precedence is a decision someone will get wrong later |
| **Timeline** — the slices in order, each with what it adds | Approval is per-slice, and the reader needs to see what he is approving into |
| **Fence** — two columns, inside and outside | Scope is the decision, and the refusals carry as much as the inclusions |
| **Exhaustive case table** | Enumeration is the cheapest way to find the case nobody planned for — it is how the held-pane question surfaced |
| **Architecture, built up in steps** | The change touches more than one system, and the reader needs to see where it lands in the whole. See below — this is the one Parker asks for by name |

Pins are cheap and worth the trouble:

```css
.pin { display: inline-flex; align-items: center; justify-content: center; width: 15px; height: 15px;
  border-radius: 50%; background: var(--s1); color: #fff; font-family: var(--mono); font-size: 9px;
  vertical-align: middle; margin-right: 5px; flex: none; }
```

## Architecture, built up in steps

The picture Parker asks for by name. Not one finished topology diagram — **the same diagram
drawn three or four times, each one adding a piece**, so the reader watches a small mechanism
become a system. HumanLayer's docs are the specimen he pointed at: two boxes and an API, then
a cloud region joins, then a phone and a coworker, then the whole board.

> *"see how the progressive small micro pieces are shown to fit together with bigger pieces?
> That is the exact thing I love to see for spec design and decision docs: where does the
> change touch other systems."*

**The frame does not move.** Every step keeps the previous boxes at the same coordinates and
the same size; new things appear around them. That is the whole trick — the eye tracks what
was added instead of re-reading the picture, and the last frame is legible because the reader
built it up rather than being handed it. If a later step needs a box to shift, shift it in
step one and leave the space empty.

Three or four steps is right. One box per step is too slow; jumping from two boxes to nine is
the finished diagram with extra frames in front of it.

The grammar, all of it plain CSS boxes — this is HTML, not SVG, because nested rounded
containers with labels are painful in SVG and trivial in CSS grid:

- **Containment is a boundary that means something.** A dashed outline is somewhere else —
  cloud, another machine, someone else's process. A solid outline is yours. Label the
  boundary in the corner in small caps mono (`☁ CLOUD`, `▣ YOUR WORKSTATION`), not in a
  caption.
- **Leaf nodes are small cards** carrying a name and a live state — `agent — auth refactor`
  with a `running` pill. Real names from the real system; a node called "Service A" tells the
  reader nothing about where his change lands.
- **Connectors are orthogonal** — horizontal and vertical segments with square corners, no
  diagonals and no curves. A 1px line the colour of the boundary it leaves.
- **One accent per frame.** The established system in `--s3` green; whatever the step is
  *adding* in `--s1` blue, so the new piece is obvious without a legend. Amber still means
  mine-to-argue-with if a step is speculative.
- **A screenshot-shaped box is allowed to be a grey blur** with a few coloured smudges where
  the panes are. Do not draw a real screen inside an architecture picture; it competes.

Caption each step with what the step *adds* and what it costs — "the daemon now answers to
two schedulers" — not with what is visible. The last frame's caption is the one that says
where the change touches other systems.

## Colour carries the same meaning as the prose

Bind it once and never re-bind it inside a picture. These are fixed across every theme; the
wallpaper changes the ground and the ink, never what red means.

| Token | Means | SVG class | Hex for SVG |
|---|---|---|---|
| `--critical` | a decision is waiting, or a claim is contradicted | `.crit` | `#d03b3b` |
| `--serious` | something failed | `.fail` | `#ec835a` |
| `--s3` | ready, or the recommended path | `.ready` | `#199e70` |
| `--s1` | structure and identity | `.struct` | `#3987e5` |
| `--s4` / `--warning` | **mine — argue with it**: an amendment, an estimate, an unbuilt assumption | `.mine` | `#c98500` / `#f5c95f` |
| `--dim` | unknown, unreadable, unavailable | `.unk` | `#8b8a80` (the theme's own muted ink, if you use the class) |

The amber column is the one that makes a brief honest. When a picture carries your additions
on top of someone else's design, put a legend line inside the picture saying so —
`yellow = the amendments` — and the reader can see in one pass which parts are his to accept
and which were already agreed.

## Draw the unknown

Unknown is not zero, and a picture is where that rule is most often broken, because an omitted
case leaves no hole. The exemplar draws a neutral UNCLASSIFIED row, writes `unavailable` into a
field of the evidence tray, and gives the origin picture three states — named, identified but
unnamed, and absent — where the plan had two.

A case you cannot classify still gets a shape. A field with no source says so on its face. A
state that is undecided is drawn and labelled undecided, in the picture, not only in the
caption.

## The gutter line

One muted line along the bottom of a diagram, smaller than the rest, carrying the thing that
would bite:

> two rankings of one pane is the failure this is most likely to ship — the tab badge is the
> one your eye checks first

> a held pane — closed, process alive — reaches none of the four inputs on the left

It costs a line and it is often the most useful sentence in the picture, because it is where
the drawing admits what it cannot show.

## A drawing reads as a measurement

Pictures raise the reader's confidence faster than prose does, including in the places where
you were guessing. Two modals repay that:

**What each picture is drawn from** — split three ways, in these words: pictures that draw
*their* plan, pictures that draw *the code* (each with the symbol, field or test that backs
it), and the pictures that are *mine*. Then the single estimated number, with its method:

> Thirteen columns per pane comes from measuring your two-column screenshot: a left bar of
> about 235 pixels against a window of about 1999, with a pane column of roughly 9.1 pixels
> per character. It is an estimate off an image, not a query to the app.

**Method and limits** — that nothing here was built, rendered by the app, or measured in it;
that font metrics and the real compositor will change how it feels; and what remains
unestablished. Confidence labels (`reference/evidence.md`) apply to a picture exactly as they
apply to a sentence.

## Budget

Extra minutes of a reader's attention are bought with pictures, and a brief that runs long on
paragraphs should be cut instead. If a figure needs three paragraphs of setup before it makes
sense, the picture is wrong — redraw it rather than propping it up.

**When you are unsure whether to draw one more, draw it.** Nobody has ever complained that a
brief had too many pictures; the complaint is always the other way. There is no count to hit
and nothing checks this — it is a standing request, and the honest test is whether a reader
skimming only the figures and their captions would come away with the argument.

A figure the reader cannot argue with is decoration. The first grill question in the exemplar
is *"do the pictures match what you had in your head?"*, and it works because every picture
has a note button on it.

## Verification

Run with the rest of the checks in `SKILL.md`:

```js
// every picture is annotatable — [present, tagged]; present > tagged is a defect
[document.querySelectorAll('figure').length,
 document.querySelectorAll('figure.notable').length]
// every figure says what it is and what it means
[...document.querySelectorAll('figure')].map(f =>
  [f.querySelector('.lbl')?.textContent.slice(0,24) || 'NO LABEL',
   f.querySelector('figcaption')?.innerText.length || 0])
// svg scales, and is described
[...document.querySelectorAll('svg')].map(s =>
  [!!s.getAttribute('viewBox'), !s.getAttribute('width'), !!s.getAttribute('aria-label')])
// nothing external — a brief must survive being emailed as one file
[...document.querySelectorAll('img, [src], link[href]')].map(e => e.src || e.href)
```

Then look at it at **968 px wide** — Parker's tiled-pane width — and at 390 px. A diagram that
only works at 1400 px is a diagram he will read once, in the wrong shape, and mark up wrong.
