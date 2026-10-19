# Docs site and the vanilla-TD info kiosk — status

**Difficulty: 6/10.** The site is public and merging `main` is the deploy, but
every file is static and a revert is one click. What can stay wrong for a while
is the authoring shape of the docs, because every page written after it
inherits it. That buys one combined plan and one approval, and the plan is
the working prototype rather than a document about one.

## What Parker asked for (2026-09-24)

1. A **docs site** at `docs.terminal-delight.brownfamilysports.com`, on
   Cloudflare, deployed aggressively. Left spine, search, top menu, light/dark,
   a CRT-warp glyph. Every page in **Brief / Story / Technical**, the
   parkerbrown-dev register system. Polish second; no AI slop first.
2. **info.html rebuilt**: short, pointing at the docs, in vanilla Terminal
   Delight style — the same shell as the docs, but a kiosk, with no search.
3. **Kiosks:** Omarchy untouched. Global stays. tv, gamba and start-crawl become
   easter eggs. Old URLs stay live.

## Decisions taken in the prototype

| Decision | Taken | Why |
|---|---|---|
| Vanilla palette | the app's default `hacker` theme | `app/src/theme.rs` embeds `themes/hacker.toml` as `DEFAULT_THEME_TOML` |
| Light mode | "paper" — the terminal printed | glass/paper pairs with the CRT toggle below |
| CRT on paper | green-bar tractor-feed paper | the texture a hardcopy terminal actually had |
| CRT on glass | scanlines, vignette, 16 s tracking band, barrel warp | 16 s is `tracking_period` in hacker.toml |
| Warp method | warp-lab approach 09: feDisplacementMap on a fixed tube, dropped while scrolling | the lab measured it at 60 fps on scroll |
| Chrome vs pane | top bar and spine flat, content pane warps | the app's rule |
| Stack | hand-written HTML + one shared shell, no framework | the kiosks already work this way; a generator comes when the page count justifies it |
| Search | titles-only palette now, Pagefind at build later | says so in the palette rather than pretending |
| Register tabs | radios + sibling selectors, per the article skill's mechanism | works with JS off, all registers crawlable |

## Found on the way

- **The warp lab's padded filter region draws a ghost.** `feImage` defaults its
  subregion to the filter region, so the lab's `x="-8%"` slides the map up and
  left, and every uncovered pixel is displaced by the full half-scale — a second,
  offset copy of the page along the right and bottom edges. Fixed in
  `td-shell.js` by pinning the region to the element. Approach 09 in
  `web-warp-lab/` still carries it.
- **At 1× the warp breaks hairlines.** Chrome samples the displacement
  nearest-pixel, so a 1px rule under it becomes dashes. Dividers on info are 2px.
  At Parker's 1.6× laptop scale it reads cleanly; on the 1× external monitor it
  is rougher, which is why docs open with CRT off and the kiosk opens with it on.

## Verified

- `node scripts/verify-site.mjs <base>` — 103 checks: clean console, no
  horizontal overflow at 1440 / 968 / 390 in glass, paper and green-bar; every
  control; copy scoped to one register; tabs with JavaScript off.
- `node scripts/verify-kiosks.mjs <base>` — 97 checks, after info left the
  Omarchy-palette family.
- `node docsite/build.mjs` — output served from its own root, all assets 200.

## Live

**https://docs.terminal-delight.brownfamilysports.com**, since 2026-09-24. Parker
added the DNS record (A → 165.245.234.21, piper-prod); the site is Caddy on that
box, the same pattern as parker.brownfamilysports.com, not Cloudflare Pages.

- `docsite/deploy.sh` builds, rsyncs to `/var/www/td-docs` (a web root nothing
  else publishes into, so `--delete` is safe), and checks five URLs answer 200.
- `docsite/deploy.sh --caddy` also installs `docsite/td-docs.caddy` as
  `/etc/caddy/conf.d/td-docs.caddy`, runs `caddy validate` on the whole config,
  and restores the previous file instead of reloading if validation fails.
- TLS is Caddy's own Let's Encrypt certificate, obtained on first reload over
  HTTP-01, as parker-job in the same zone has. It holds with the record
  DNS-only or proxied.
- The neighbouring sites answered 200 before and after the reload.

## Round two (2026-09-25), from Parker's review

- **No second top bar.** The family strip (`assets/kiosk-chrome.js`) stacked a
  bar over the Omarchy kiosk's own navigation: *"the double topper bar for
  other kiosks is a NO"*. The file now only boots the theme; no page changed.
  The agent wall's palette picker lived in that strip and went with it.
- **One top bar on info and docs:** Overview · Docs · Omarchy · Global.
- **Install, not Download.** *"It is poor form to trap someone without their
  reading first."* The bar's button and the hero open the new Install page.
- **`cargo install terminal-delight` does not exist**, so it is on no button.
  The crate is unpublished (`publish = false`, gpui is a path dependency). The
  Install page says why. A one-line `$` install would need an install script
  served from the docs domain — not built, Parker's call.
- **Warp half as strong again:** `CURVE` 0.26 → 0.39.

## Round three (2026-09-25): warp the glass, never the text

At the stronger curve the feDisplacementMap warp *"demolished"* the text.
Chrome evaluates that filter nearest-pixel at screen resolution, so glyphs,
rules and card borders break wherever the displacement crosses a whole pixel.
Laying the content out at 2× and scaling it back was tested and does not help;
the filter is still computed at screen size.

The answer was already written down in
`~/BROWN-FAMILY-SPORTS/Software/curved-glass-web/docs/LESSONS.md`: *"Warp the
GLASS, never the live text."* The page is now flat under the tube, and the
glass is bent through the same barrel at the same 0.39 curve: bowed scanlines,
a cushion-shaped screen edge, a rim shadow and the top-left glare the hacker
theme calls `screen_glare`, drawn once per size into one canvas. Nothing is
resampled and scrolling costs nothing. `verify-site.mjs` asserts the page
never carries a filter.

The true warp of live text needs the HTML-in-Canvas API (`texElementImage2D`).
It is in Chrome 152 and Chromium 151 on this box, but only behind
`--enable-blink-features=CanvasDrawElement`, so visitors do not have it.

## Round four (2026-09-25): the page bends, the text survives

Glass-only was rejected: *"nothing is curved though -- the warped border
doesn't curve anything inside of it"*. curved-glass-web's engine
(`packages/webgl-warp`) never bends the DOM. It draws content into a canvas
and bends that in a shader with LINEAR sampling. Our content is HTML, so
`assets/td-glass.js` draws it into the canvas by snapshot: an SVG
foreignObject of the tube with every stylesheet inlined (fonts as data:),
2× resolution, one tall texture, and the shader samples the scrolled slice.
A throwaway spike proved it before the build; Parker: *"heck to the yes!
Send it!"*

- Clicks are mapped through the barrel to the element drawn under the
  pointer. Only pointer-made clicks are redirected. Redirecting a label's
  own follow-up click cancelled the radio, and the verifier caught it.
- Hover re-snapshots when the pointer reaches something clickable.
- Animated things are `<canvas data-glass-live>` islands, copied into the
  texture as they draw. The first is the Global card's title: the app's nine
  names every 1.5 s, through a burst of tracking-bar squiggle.
- Fonts are self-hosted (`assets/fonts`, OFL, 80 KB). The four pages no
  longer load anything from Google, and their CSP is `font-src 'self'`.
- Fallback: no WebGL2, a pane under 600px, paper, or a page taller than the
  GPU's texture limit gets the flat glass overlay.
- `verify-site.mjs`: 259 checks. The corner case, a 10px channel dot where
  the page under the pointer is something else, is proven by mutation. With
  the redirect off exactly that check fails.

Not done: tiling for a docs page taller than one texture (Chrome allows
8192–16384px). Hover redraws are about 100 ms behind the pointer.

## Round five (2026-09-25): the whole spine, written from the code

Parker: *"default to CRT off — let's get all this deployed and cleaned up —
then you will basically work all night on just building out the
documentation … keep things pretty dry and give it lots of love!"*

- CRT defaults to off. The kiosk merged in #790 and again in #794; the docs
  site deployed after every batch. **29 of 29 pages are live**, 226 search
  entries, 1366 internal links, none broken.
- Every page was written from a fact sheet traced to file and line in main at
  `d7cf383`, not from `docs/features/`, which is stale in dozens of places.
  The drift is filed as #803 (repo docs) and #804 (TDSP/TDAC spec).
- Stories where a metaphor earned one: Sessions, the Workbench, and the agent
  wall's three bars (a desk: how covered, how long at it, how much still for
  the job). Every other page is Brief and Technical.
- Pictures generated from the shader math rather than drawn by hand: the
  barrel warp at 0 and 1.43 (`docsite/tools/barrel-figure.mjs`) and the text
  crawl (`docsite/tools/crawl-figure.mjs`), so each caption's numbers and its
  drawing come from the same arithmetic. The skins figure is hand-built HTML
  and its caption says so.
- The build's lint refused a draft that opened with a bold label. It was
  rewritten, and the gate did its job.
- Five bugs found on the way, each filed with proof and linked from the page
  it affects: ctl deletes a live window's socket on a slow reply (#796,
  reproduced with a fake window), `<dir>` ignored on the hosted path (#798),
  a missing percent reads as 0% spent (#799), Δ0 for an unknown count (#800),
  the jiggle only hops one way (#801). Plus #802: the launcher's project
  roots are one machine's folders, which a packaged build can't ship.
- Corrected on pages already published: the install page's bell (one
  built-in ping, no sound files) and patch count (six), the info kiosk's
  Codex card (Codex draws no bars), three real project names under a "made
  up" caption, the kiosk's tracking-band period, and the workbench's
  "unweighed" and "three transports".
- Keys are spelled the way the app's help spells them on every page.

## Open — Parker's call

- The agent-wall kiosk (`agents.html`) was not named in the scrap list.
- Whether to tile the page texture. Nothing falls back today: the tallest
  register, the workbench's Technical, is 6,040px, and the engine drops from
  2× to fit the GPU's limit (1.36× under an 8,192 limit, full 2× under the
  more common 16,384). A page would have to pass 8,192px at 1× to lose the
  glass. Measured with `crt: on` in headless Chromium, 2026-09-25.
- The remaining docs commits (round five after #794) need one more PR.

## Post-hoc score

**7/10, one above the prediction.** The authoring shape held: the generator,
the registers and the page format carried twenty-six pages without a change.
What stayed wrong longest was the warp: three approaches before the page bent
and the text survived. The other surprise was the source material. The
repository's own docs could not be copied, so every page needed its facts
traced to the code, and that tracing turned up six issues' worth of bugs.
