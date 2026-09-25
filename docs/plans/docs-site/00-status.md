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

## Open — Parker's call

- info's `/docsite/` links switch to the subdomain when this branch merges.
- The page footer's "Edit this page" points at `main`, where the file does not
  exist until the merge.
- The shared kiosk strip (`assets/kiosk-chrome.js`) still lists tv, gamba and
  crawl. Removing them changes the strip on the Omarchy page too, so it waits.
- The agent-wall kiosk (`agents.html`) was not named in the scrap list.
- The next pages to write, in the order the spine lists them.

## Post-hoc score

*(to be filled in when this lands)*
