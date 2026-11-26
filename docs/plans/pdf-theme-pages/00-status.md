# A PDF's pages wear the theme, with a Paper chip

**Difficulty: 5/10.** One step added to the tiles the PDF square already draws
(`docs/plans/pdf-in-the-square/`): each tile is recoloured as it lands, off the
main thread. What can go wrong quietly is a picture's place landing in the wrong
spot, and that shows the first time a figure page is opened. Unwinding it is one
module (`app/src/docview/pdf_look.rs`) and the hooks into `pdf.rs`. One combined
plan, this page.

**Approval:** Parker, 2026-11-26, in the request itself: *"alt+click in TD to
open a PDF ... we want it to inherit the theme and go dark mode by default (click
button for paper) just like in https://halo.brownfamilysports.com/library/ ...
inherit theme via colour mapping + print pictures."* The calls below are the ones
he would otherwise have been asked.

## Decisions made without him

| Question | Call | Why |
|---|---|---|
| What mapping? | **The Halo reader's, pixel for pixel**: invert, turn the hue half round, stretch into surface → text, paper 65% opaque | He pointed at that reader. Its SVG filter is three linear steps, so the same arithmetic runs on a tile's pixels (`pdf_look::map_pixel`). |
| Which theme colours? | **`surface` for paper, `text` for ink** | The Halo reader uses its lifted ground (`bg2`) and its foreground; `surface` is the lifted ground here. |
| Where are the pictures? | **`mutool trace`, once per reading of the file, in a child process** | poppler cannot say cheaply: `pdftohtml -xml` decodes every image to report it, measured at 108–116 s for a 48-page, 73-image paper. `mutool trace` lists the same placements in 0.79 s without decoding. It keeps the in-the-square plan's rule that a hostile file crashes a child, not the window. |
| No `mutool`? | **Pages still go dark; pictures go with them** | Optional, found on PATH like poppler. Nothing is known, so nothing is kept. |
| A first paint before the trace answers? | **Themed tiles wait up to 1.2 s for it**, then draw without | So a photograph is not first drawn as a negative. A short paper traces in under 0.1 s. |
| A scan? | **An image covering 85% of its page or more is the page**, recoloured with the text | Otherwise every scanned PDF would stay white. |
| Paper chip where? | **Top right of the PDF itself**, beside the page counter's corner, lit while on | The strip is drawn in three places outside the PDF backend; a chip inside it is one file. He asked for a click. |
| A key for it? | **Not now** | Keyboard ownership is a verified subsystem (`verify/README.md`); a click is what was asked for. |
| Remembered? | **Per open document; every PDF opens dark** | "dark mode by default". |
| A theme switch? | **Every tile is drawn again**, the old ones covering their pages until the new land | The same rule a re-read file already follows, through a second counter on the tile key so a switch never drops a file read in flight. |

## Status

- [x] `pdf_look.rs`: mapping, picture places in a tile, `mutool trace` parser, and tests (one against the real `mutool` where it is installed)
- [x] `pdf.rs`: look counter on tiles, trace per reading, recolour on landing, Paper chip
- [x] Gates, 2026-11-26: `cargo fmt --check` clean; `cargo clippy --locked -- -D warnings` clean; `cargo test --locked --no-fail-fast` 3,629 passed with one failure, the document-view file scan, which did not yet list `pdf_look.rs`; listed, its 17 tests pass
- [x] Real pixels: `pdf_look::tests::sample_page` (run by hand) drew page 2 of arXiv 2211.11031 through poppler, `mutool trace` and the recolour: cream text on the theme's surface, the figure kept as printed, the green and red citation links still green and red
- [ ] Seen in a real window: the chip's click and a theme switch

**Post-hoc grade:** 5 held. The surprise was the picture positions: poppler's own route costs two minutes a paper, and `mutool` had to come in.
