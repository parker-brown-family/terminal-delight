# A PDF opens in the floating square

**Difficulty: 5/10.** One new kind of document behind the document view's
existing one-backend-per-kind door, the door the video came through three days
ago. What can go wrong quietly is the same thing that could with the video:
every page drawn is a texture in the window's atlas, a PDF can have hundreds of
pages, and a leak or an oversized texture shows in no unit test. It is below the
video's six because nothing new runs inside TD's process: poppler draws each
page in a child process and hands back plain pixels. Unwinding it is deleting
one variant and two files. One combined plan page, this one.

**Approval:** Parker, 2026-09-28, in the request itself: *"We are going to get
PDFs into our Alt left click so that we can open them inside of the Terminal
Delight window."* The decisions below are the ones he would otherwise have been
asked, each with the call made, as the video plan did.

## What he asked for

- **A PDF opens where images, HTML, Markdown and video already open**: Alt+click
  into the floating square, Ctrl+Alt+click beside.

What the survey found before any code: every surface that opens a document asks
one question, `docopen::drawable_document`, before it opens anything — the
Alt+click on a path, the bench's artifact cards, the attention rail's
deliverable row and `open_document`. So a PDF that answers that question is on
every one of those surfaces at once, as the video was.

## Decisions made without him

| Question | Call | Why |
|---|---|---|
| What draws the pages? | **poppler's `pdftoppm`, run as a child process per tile**, with `pdfinfo` reading the page count and sizes once | poppler is what most Linux PDF viewers draw with, and it is on this machine (26.08). A child process means a malformed or hostile PDF crashes poppler, not the window. The alternatives each miss something: headless Chromium draws its own PDF viewer's chrome and would come back as screenshots of a viewer; libpoppler loaded with `dlopen` puts the parser inside TD's process and brings cairo with it; pdfium is not installed; a pure-Rust renderer is a new crate and young. |
| Linked or found? | **Found on PATH when the first PDF opens**, looked for again after thirty seconds when missing | As with Chromium and libmpv: a machine without poppler runs TD as before, and a PDF there goes to the desktop with a chip ("no poppler · opened on the desktop"). |
| How are pages laid out? | **One continuous column**, every page scaled by the same amount so the widest fits the view at 100%, centred, with a gap between pages | The way a PDF reader's "fit width, continuous" mode reads. One scale for every page keeps a small page small, as it is in the file. |
| Zoom? | **The reading ladder**, 50% to 300%, the one HTML and Markdown step along, 100% being fit-to-width | Same controls on the strip, same ctrl+wheel, same `0` `1` `+` `-` on the Document face. Zoomed past the width, the wheel's sideways turn and the arrows pan. |
| Sharp when zoomed? | **Drawn again at the new size**, in tiles of at most 2,048 device pixels a side, only where the view is looking | A page at 300% on a wide HiDPI pane is over 7,000 pixels wide, past what a GPU texture can be. poppler draws a cropped region in proportion to its area (a 2,048² tile of a 4,096-wide page in about 120 ms against 300 ms for the whole), so a tile costs what it shows. Until the new tiles land, the old ones are drawn stretched, soft for a moment, as a page's are. |
| How many on the GPU? | **The tiles in view and half a view either side**, everything else given back | A letter page at reading size is about 8 MB. Closing the square gives back every one. |
| Which page? | **A small `3 / 12` at the bottom right**, the page under the middle of the view | A column of identical pages gives no other sign of where you are. |
| Follows the file? | **Yes**: a PDF rewritten on disk is read again, keeping the reader's place | Agents regenerate PDFs (the invoice skill writes one), and the square should show the new one without a close and reopen. |
| Which files? | **`.pdf` whose first bytes are `%PDF-`**, and a file with no extension that starts with `%PDF-` | A log named `.pdf` would open a square that never draws. Unlike an MP4's `ftyp` box, `%PDF-` belongs to nothing else, so an extensionless file can be recognised by it, as a screenshot with no suffix is. |
| Locked with a password? | **The square says so**, and that Ctrl+click opens it on the desktop, which can ask | TD has nowhere to type a password, and opening another window unasked is a surprise. |
| Links, text selection, search, notes? | **Not in this change.** A press or drag on a page grabs and moves it, as on a picture | poppler's command-line tools do not report where a page's links sit without a second, heavier pass. A PDF takes no notes, as a picture takes none. |

## Slices

- [x] 1 · `DocKind::Pdf`, recognised by name and `%PDF-` (`docopen.rs`)
- [x] 2 · `docview/poppler.rs`: find the tools; read `pdfinfo`; draw one tile with `pdftoppm` into bytes gpui draws, with a time limit, killed the moment nobody wants it; every failure a sentence. A test drives the real poppler over a PDF built so that each of the two traps (crop box, rotation) shows in its pixels; CI now installs `poppler-utils` so it runs there, and fails under `CI` rather than skipping.
- [x] 3 · `docview/pdf.rs`: the backend — the column's layout and which tiles it wants, pure and tested; tiles fetched nearest first, given back when they leave; scroll, pan, drag, zoom, page counter; follows its file; a restart puts the reader back where they were; `#page=N` from a document's link
- [x] 4 · The router: `engine_refused` hands a PDF to the desktop when there is no poppler. Two pane tests: that refusal, and a real Alt+click that reads both pages, draws the one in view, lands on page 2 from a `#page=2` link, and drops the view on Escape.
- [x] 5 · Docs: panes-and-tabs, workbench, TDSP artifact section, `open_document`'s description, changelog
- [x] 5b · The document view's guards scanned a hand-written list of files, and the two new ones were not on it. Both are now, and `every_document_view_file_is_scanned` fails the next time a file under `docview/` is left off.
- [x] 6 · Verified in a hidden window. `scripts/pdf-check.sh` builds a twelve-page PDF (ten letter pages, a landscape page, a page turned 90°), drives the window through the control socket, and passes all eleven of its checks: all twelve pages read; the page in view drawn, the slowest tile 53 ms; scrolled to the end, every page drawn on the way and never more than two tiles on the GPU; the file rewritten to ten pages while open, read again with the reader kept on the last page; closing releases the view; twenty opens and closes leave the window's GPU memory at 186 MiB, where the first close left it (a leak of the two tiles each open draws would have added about 140 MiB); `ctl doc beside` draws it on a Document face; nothing reached the desktop. Photographed at the first page, the last (the turned page shown turned), after the rewrite, and on the face. Separately, `pdfinfo` lists every page of a 2,002-page PDF in 49 ms, so reading the sizes first costs nothing a reader would notice.
- [x] 7 · Land: PR #879 merged as `f71ebf9` after CI, where both real-poppler tests ran and passed under both cores against Ubuntu's poppler 24.02, so the two traps hold on 24.02 as on 26.08. The whole suite was green on the merged tree (2,007 passed), and the build is installed as `td-f71ebf9-pdf-square` with the launcher repointed from `td-53f476d-workbench-turn-fix`, its ancestor. #879 changed no host wire file, so a new window pairs with the running host as before. Running windows keep their old binary until relaunched.

**Turned out to be: 5/10.** The estimate held. poppler behaved as measured
before any code, and the backend compiled and drew on its first run. What the
plan did not see was in the tests: the document view's guards read a
hand-written list of files, and neither new file was on it, so the rule that no
backend registers a mouse handler was not being checked against the PDF
backend at all until that list was made to fail on a file left off.

## Not done

- Links inside a PDF, text selection and search.
- One scale for every page means a portrait document with one landscape page
  in it shows its portrait pages at about three-quarters of the width, as
  Chrome's viewer does. Fitting each page to the width on its own would fill
  the view and make neighbouring pages different sizes; worth asking once
  someone has read a real mixed document this way.
- The rail's row label says `open` for a PDF, as it does for a picture and a
  video: only HTML and Markdown are named there today.
