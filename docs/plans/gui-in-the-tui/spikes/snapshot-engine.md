# Snapshot engine

A measurement spike, run on 2026-09-24, to settle how Terminal Delight shows Parker's HTML briefs in a pane: as an image of the page drawn by headless Chromium, with TD drawing the note affordances itself and writing notes back into the file so the same file still works in a browser. Nothing in TD was built or changed. Every number below comes from a script in `snapshot-engine/` next to this file, run on the corpus of 119 HTML files in `~/Work/reports` and `~/Work/terminal-delight/reports` plus Parker's saved brief `~/Downloads/2026-09-24-gui-in-the-tui.html`. Every file the scripts wrote to is a throwaway copy under `/tmp/claude-1000/td-snapshot-spike/`.

Each claim carries a label: **measured** (a script produced the number), **inferred** (follows from code or from measured numbers by arithmetic), **hunch**, or **not established**.

Machine: Legion 7, Ryzen 9 5900HX (16 threads), 31 GiB, NVIDIA GeForce RTX 3080 Laptop GPU (driver 610.57.04, the adapter TD's own log names), laptop panel 2560×1600 at scale 1.6. Chromium 151.0.7922.173 (`/usr/bin/chromium`), playwright-core 1.61.1, Node 26.7.0. Rust probe built against the same wgpu git revision (`357a0c5`, v29.0.3 from zed-industries), `image` 0.25.10 and `png` 0.18.1 that TD's `Cargo.lock` resolves.

## Summary

1. Anchor ids stay the same at every width tested. In headless Chromium at 968 CSS px × 1.6, the saved brief's own notes.js tagged 53 elements, and all 9 keys of its `report-notes` island were among them with matching titles. The ordered id list was identical at five width/scale combinations, and identical at 968 and 600 wide in all 93 notes-bearing briefs. Extracting ids, titles and page rects took 7.4 ms warm (155 ms on the first call, which forces the first layout). [measured]
2. TD has to run each brief's own notes.js rather than a reimplementation of it. The corpus carries 10 different notes.js copies: 3 of the skill's 4 releases verbatim (81 briefs) and 7 hand-edited forks (12 briefs). Running today's notes.js in place of each brief's own copy gave identical ids in 91 of 93 briefs; the other two gained 1 and 4 extra anchors, and a note TD put on one of those would be invisible in a browser. [measured]
3. Rewriting only the island's JSON and the trailing `READER NOTES` comment round-trips. It added 240 bytes to the 135,551-byte saved brief in 1.4 ms and left every other byte identical. Reopened with fresh storage, the badges sat on the right 10 elements, one of them inside a modal. The island bytes equal what notes.js's own save writes for the same map, and concurs round-trip the same way through `report-concurs`. [measured]
4. Browser localStorage beats the file, and an empty map beats it too. With an older `notes:<file>` entry present, the browser shows the older notes and none of TD's. With `{}` stored (what the ✕ button leaves behind) it shows zero notes on every load. `file://` is one origin, so two copies with the same basename share one entry. Parker's Chromium profile holds 22 distinct `notes:` keys, `notes:2026-09-24-gui-in-the-tui.html` among them. [measured; how many of those keys are still live was not established]
5. At 968 × 1.6 the corpus runs to a median of 6,471 CSS px (10,354 device px), p90 17,680 device px, and a maximum of 25,902 device px. 79 of 119 pages exceed 8,192 device px, 19 exceed 16,384, none exceed 32,768. [measured]
6. TD's GPU takes a 32,768-px texture. wgpu on the RTX 3080 Laptop reports `max_texture_dimension_2d` 32,768, and gpui_wgpu requests exactly that (`downlevel_defaults().using_resolution(adapter.limits())`). Chromium's WebGPU reports 16,384 for the same GPU, and 8,192 from the SwiftShader fallback headless uses by default, so neither browser figure is the one TD sees. [measured]
7. A whole long brief is a 1549 × 25,902 PNG of 4.6 MB that decodes to 153 MiB of RGBA in 259 ms and uploads in 43 ms. Rendering it takes 2.2 s cold and 1.5 s on a page already loaded. That fits in one texture and still argues for tiles: tiles 2,048 device px tall are 12.1 MiB each, and five resident come to 61 MiB. [measured sizes and times; the tile plan is inferred]
8. A viewport capture over the DevTools protocol takes 45 ms (PNG optimised for speed) to 73 ms at 1549 × 2240, plus 10 ms to decode and 4 ms to upload: about 60–90 ms a frame, or 11–16 fps. That is enough to refresh after a scroll settles and far short of 60 fps, so scrolling has to move already-uploaded tiles on TD's side. A pane resize re-lays out, re-extracts anchors and captures the first viewport in 70–93 ms. [measured]
9. Each modal can be rendered as its own image with its own anchors. Opening each of six modals by script and capturing it took 265–574 ms per modal through Playwright's element screenshot and 115–152 ms through a raw DevTools clip. 30 of the corpus's 285 modals scroll inside themselves at a 1400-px viewport, and one image covers those only if the viewport is grown first (about 0.7 s each). 6 briefs ship modal buttons with no script behind them. [measured]
10. A live engine could hand saving to the page, and it should not. notes.js's "save into file" is interceptable as a Playwright download containing the whole file, but those bytes are a DOM re-serialisation: 41–295 changed lines on an authored brief, one more stale `READER NOTES` comment per save, and a note containing `</script>` leaks into the page and loses every note. Screencast frames arrive at 968 × 1400, which is CSS resolution rather than 1.6×. An unanswered `confirm()` blocks the page's main thread. [measured]

Two more findings came out of the work without a question asking for them. From the third load in one browser context, the saved brief lays out 29 CSS px taller and the longest brief 17 px taller, reproducibly, so anchors and pixels have to come from the same page instance in the same pass. [measured; the cause is not established, and font-fallback caching is a hunch] And the page's own notes UI is baked into the image: the fixed notebar lands over body text at the bottom of the first viewport, and the note-count badges are drawn in. Hiding `.notebar, .note-btn, .concur-zone` with `visibility: hidden` before capture moved no anchor in any of the 119 files. [measured]

## 1. Anchors

Method: `q1-anchors.mjs` loads the saved brief in a fresh context at 968 × 1400 CSS px, `deviceScaleFactor` 1.6, waits for `load`, and evaluates one function that returns each `.notable` element's `data-nid`, `data-ntitle`, tag, owning dialog and `getBoundingClientRect()` plus scroll, which gives page coordinates. The island is parsed from the file's bytes and compared. The same file is then loaded at 600 × 1.6, 968 × 1, 1400 × 1.6, 2000 × 1 and 390 × 3. `q1-corpus-anchors.mjs` repeats the width test on every brief.

| | Saved brief | Corpus (93 notes-bearing briefs) |
|---|---|---|
| Anchors tagged | 53 (40 in the page, 13 inside closed modals) | min 5, median 39, max 81 |
| Island keys found among the anchors | 9 of 9, titles identical | 0 orphaned keys in the 5 briefs that have notes |
| Ids identical across widths/scales | yes, at all 5 variants | 93 of 93 at 968 vs 600 |
| Page load (`load` event) | 64 ms | p50 99 ms, max 208 ms |
| First extraction call | 155 ms | p50 22 ms, max 178 ms |
| Warm extraction (median of 19) | 7.4 ms | — |
| Anchor list as JSON | 8,745 bytes | — |

[all measured]

The ids are width-invariant by construction, since `titleOf()` reads `textContent` and never layout, and the measurement confirms it. The rects are not invariant: the same brief is 15,959 CSS px tall at 968 and 18,519 at 600. Anchors inside a closed `<dialog>` have zero-size rects. They are known and keyed but not placed, and the engine should report them with no rect rather than a rect of zero. [measured]

### Which notes.js a brief carries

`corpus.mjs` hashes every inline notes.js and `versions.mjs` compares the copies against the four commits of the skill's `assets/notes.js`. [measured]

| notes.js | Briefs | Concurs? |
|---|---|---|
| `250188f` (2026-09-24, concur stamps) | 1 (`2026-09-24-concur-stamp.html`) | yes |
| `b689671` (figures become targets) | 60 | no |
| `eca5cb8` (2026-09-03) | 20 | no |
| 7 hand-edited forks, 2–241 lines from the nearest release | 12 | no |

So 1 brief can show concurs and 92 cannot. 8 briefs also override `NOTES_TARGETS` wholesale, and 2 set `NOTES_FILE` to a name other than their own filename (`2026-09-09-client-server-review-reconciled.html` and `…spine-dry.annotated.html` both point at their originals). Parker's saved brief carries a figures-era copy with no concurs. [measured]

The comparison against today's notes.js used stripped copies of each brief (own notes.js removed, today's injected after load): 91 identical, and `2026-09-02-animated-agent-avatars.html` (+1) and `2026-09-12-chrome-skin-layer.html` (+4) gained anchors without losing any. [measured] A Rust port of the tagging rules would have to track 10 variants and 8 overrides, and it would fall behind the next time the skill changes, as it did today. [inferred]

## 2. Round trip

Method: `notes-writer.mjs` holds the write rule as reference code for TD's Rust side. It works on bytes, finds markers in a latin1 view so string offsets equal byte offsets, and rewrites exactly three spans: the inner text of the first `report-notes` script element, the inner text of the first `report-concurs` element (or inserts one straight after the notes island when there are concurs to write and none exists, which is where notes.js's own save puts it), and the last `<!--\nREADER NOTES —` comment (or inserts one before the last `</body>`, or at end of file). `q2-roundtrip.mjs` exercises it on copies and reopens each copy in a fresh browser context. `q2-island-agreement.mjs` checks the byte finder against the browser.

| Case | Result |
|---|---|
| Saved brief: add one note to a page figure, one inside a modal, delete one | +240 bytes, 1.4 ms; bytes outside the edited spans identical; reopened with fresh storage: count 12, badges on 10 elements, both new ones present (the modal one too), the deleted one gone, no page errors |
| Pristine brief (empty island, no comment) with a hostile note: `</script><b>…` then `-->` then `<!--<script>`, Unicode, a newline | round-trips exactly; nothing leaks into the page; comment inserted before `</body>` |
| Concur brief: notes + two concurs | `report-concurs` replaced in place; stamps drawn on both decisions; concur count 2; map header reads `1 notes on 1 elements · 2 concurs.` with `✓ concur` on both lines |
| Concurs written into a brief whose notes.js predates them | island inserted, page shows no concur zones and no stamps; the 9 notes still show |
| Byte finder vs `document.getElementById` | picks the same element in all 94 islands in the corpus (93 notes, 1 concurs) |
| Island bytes vs notes.js's own save, same map | identical (saved brief) |

[all measured]

### localStorage precedence

notes.js reads `N = JSON.parse(localStorage[KEY] || 'null') || baked`, and concurs follow the same rule under `concurs:<file>`. Measured on the TD-written copy:

| Stored in localStorage before opening | What the browser shows |
|---|---|
| nothing | TD's notes: 12 notes on 10 elements |
| an older one-note map | the older note only; TD's 12 are hidden |
| `{}` | zero notes |
| `concurs:` with a key matching no anchor | no stamps, and a concur count of 1 |
| `concurs:` `{}` | no stamps, count 0 |

A copy with the same basename in another directory read the first copy's entry (`location.origin` is `file://` for both). [measured] In Parker's real Chromium profile (`~/.config/chromium/Default/Local Storage/leveldb`), a string scan of key names found 22 distinct `notes:*` keys, including this brief's. Google Chrome's Default profile has 3. [measured; leveldb files can still hold deleted keys, so how many are live, and what they hold, was not established] 8 basenames exist in both corpus directories, as separate copies that share those entries. [measured]

What that means for a TD write: if Parker has ever opened the brief in Chromium and touched a note, his browser keeps showing its own map, and a later "save into file" from the browser writes that map out (to `~/Downloads`), without TD's notes. TD cannot write into the browser's storage. The real fix is on the skill side: a revision or timestamp in the island that notes.js compares against its stored copy, preferring the newer. Until then TD can only detect and warn. [inferred]

### The smallest safe write rule

In order:

1. Take anchors from the engine's render of the current bytes, never from a cache: a content hash of the bytes rendered travels with the anchor list.
2. At write time, re-read the file and compare its hash with the rendered one. If it changed, re-render and re-anchor before writing.
3. Parse the island fresh from those bytes and apply TD's change as a delta (append one note, delete one note, toggle one concur). Never overwrite with a map TD held in memory.
4. Serialise as `JSON.stringify(map, null, 1)` does, then escape `</` as `<\/` and the `<` of every `<!` as its JSON unicode escape, backslash-u-003c (still valid JSON, same strings when parsed; `islandJson()` in `notes-writer.mjs`). Write concurs only when the brief's notes.js supports them, which the engine reports per anchor.
5. Label the map header with the brief's `NOTES_FILE`, not TD's path basename (they differ in 2 briefs). Replace the last `READER NOTES` comment, else insert one before the last `</body>`, else append. Inside it, break `-->` and `--!>` with a space. Parker's own `---` stays as written.
6. Verify in memory: the island re-parses to the intended map, and every byte outside the edited spans equals the original.
7. Keep a copy of the original bytes in TD's state directory (a short ring per file), then write a temporary file in the same directory, fsync it, and rename it over the original, preserving the mode. The home volume is btrfs, so the rename is atomic. No brief is a symlink or hardlink today. [measured]
8. Re-render the written file in the engine and confirm the island parses and the badge set matches: about 100 ms of load, and it catches every case the byte rules miss. [inferred cost from the measured load times]

### Brief shapes where it breaks

Counts measured:

- No island (26 files): refuse to write; see section 6.
- Two islands (`2026-09-15-one-terminal-one-pane.html`, the template pasted twice): write the first, which is the one the browser reads.
- Minified (10 files with lines up to 1.18 MB): the byte finder located the island in every one of them and agreed with the browser. The rule does not depend on line structure.
- Island after notes.js: notes.js reads the island at the top of its script, before the rest of the document is parsed, so notes baked below it would never load. It is before notes.js in 93 of 93 briefs; the writer should refuse if not. [measured position, inferred effect]
- An island-shaped string inside an HTML comment or a `<pre>` sample placed before the real island would fool a regex. No such case exists today, and step 8 catches it.

## 3. The page image

Method: `q3-render.mjs` measures six briefs: the shortest, one near the median height, the saved brief, a long one, the tallest in the corpus, and the heaviest by bytes (a 3 MB brief of embedded images). Cold is a fresh browser launch through the full-page PNG; warm is a new page in a running browser; "again" is a second full-page capture of a page already loaded. Medians of 3. The Rust probe (`rust-probe/`, `cargo run --release --offline -- decode …`) times `image::load_from_memory` to RGBA8 plus the RGBA-to-BGRA swizzle gpui's image path performs. Medians of 5.

| Brief | CSS px tall | Device px | PNG | RGBA | Cold | Warm | Again | Decode | Swizzle |
|---|---|---|---|---|---|---|---|---|---|
| short (`concur-stamp`) | 1,853 | 1549×2965 | 0.41 MB | 17.5 MiB | 631 ms | 317 ms | 179 ms | 28 ms | 3 ms |
| median (`a-closed-pipe-is-not-a-panic`) | 6,531 | 1549×10450 | 1.60 MB | 61.7 MiB | 1,100 ms | 790 ms | 576 ms | 103 ms | 9 ms |
| saved brief (`gui-in-the-tui`) | 15,959 | 1549×25534 | 5.23 MB | 150.9 MiB | 2,192 ms | 1,881 ms | 1,570 ms | 261 ms | 23 ms |
| long (`the-rodeo-checklist`) | 15,410 | 1549×24656 | 4.34 MB | 145.7 MiB | 2,080 ms | 1,786 ms | 1,461 ms | 249 ms | 21 ms |
| tallest (`microsurvey-invoicing-harness`) | 16,189 | 1549×25902 | 4.60 MB | 153.1 MiB | 2,186 ms | 1,874 ms | 1,534 ms | 259 ms | 23 ms |
| heaviest bytes (`theme-foundry-interview`) | 9,734 | 1549×15574 | 3.02 MB | 92.0 MiB | 1,820 ms | 1,554 ms | 1,179 ms | 162 ms | 14 ms |

[measured] Browser launch alone is 155–163 ms. The Chromium process tree with one brief loaded occupies 402–570 MiB PSS under headless defaults, and 265 MiB with the Vulkan flags of section 7. A full-page PNG's bottom 1400 px matched a clip of the same region pixel for pixel on five of six briefs. The short one differed in 0.07 % of pixels, and its PNG size changed between captures (423,722 then 412,839 bytes). [measured] Something on that page animates. [hunch]

### GPU limits

`q3-gpu-limits-browser.mjs` asks WebGPU from a `file://` page, since `about:blank` is not a secure context. Headless defaults give Google SwiftShader (`isFallbackAdapter` true), 8,192. With `--use-angle=vulkan` and the Vulkan features it gives NVIDIA Ampere, 16,384 on the low-power request and no high-performance adapter. The Rust probe (`… -- limits`) enumerates Vulkan through TD's wgpu revision: NVIDIA GeForce RTX 3080 Laptop GPU, `max_texture_dimension_2d` 32,768, and gpui's requested limit also 32,768. Only that one adapter enumerated. Uploading a BGRA texture and waiting for the GPU: 1549×2240 4.3 ms, ×4096 7.6 ms, ×8192 14.1 ms, ×16384 26.8 ms, ×25902 43.2 ms. A 1549×40000 texture fails validation ("exceeds the limit of 32768"). [measured] vulkaninfo is not installed, so the wgpu probe stands in for it.

### Tiling

One texture per page fits every brief today, but it holds 61–153 MiB of VRAM per open brief (a floating square and a split can both be open), and it stops working at a page height of 20,480 CSS px (32,768 / 1.6). The tallest brief is at 79 % of that. [inferred from measured numbers] The proposal:

- Tiles span the full layout width (1549 device px at 968 CSS) and are **2,048 device px tall** (1,280 CSS px). One tile is 12.1 MiB BGRA. A 2,240-px viewport spans two or three tiles.
- Keep the visible tiles plus one above and one below resident: at most 5 tiles, 61 MiB. [inferred]
- Keep every tile's PNG on the CPU side, 0.2–0.8 MB each and 0.4–5.2 MB for a whole page, so bringing a tile back costs a decode (about 20 ms, half the 40 ms measured for a 4,096-px tile) and an upload (about 4 ms), not a re-render. [inferred]
- Capture the visible viewport first, then tiles outward in the background. A 4,096-px tile captured over DevTools took 108 ms (PNG optimised for speed); a 2,048-px tile should be around half. The 13 tiles of the tallest brief come to roughly 0.7–1 s, about what one full-page capture costs, except that the first pixels arrive in under 100 ms. [inferred]
- Scrolling moves resident tiles on TD's side with no engine round trip per frame.

### Visible region only

`q3-capture-cdp.mjs` calls `Page.captureScreenshot` directly on the tallest brief at ten scroll positions. [measured]

| Encoding | Viewport 1549×2240 | Bytes | Tile 1549×4096 | Bytes |
|---|---|---|---|---|
| PNG | 72.9 ms | 211 KB | 167 ms | 396 KB |
| PNG, `optimizeForSpeed` | 44.7 ms | 269 KB | 108 ms | 502 KB |
| JPEG q90 | 45.4 ms | 225 KB | 108 ms | 414 KB |
| WebP q90 | 131.6 ms | 126 KB | 270 ms | 241 KB |

Playwright's own `page.screenshot` of the viewport took 146–163 ms, most of it Playwright's waits. Decoding the viewport PNG in Rust takes 10.2 ms and uploading it 4.3 ms. End to end that is about 60–90 ms per visible frame, which is fine for re-rendering after a scroll settles, after a note is added, or after a resize (`q3-resize.mjs`: 70–93 ms to re-layout at a new width, re-extract all 53 anchors and capture the first viewport), and too slow to drive scrolling. [measured parts, inferred sum] JPEG is as fast as fast PNG but blurs text edges. The spike did not test that on Parker's display, so fast PNG is the default. [hunch]

### Layout drift across loads

`q3-layout-drift.mjs` and `q3-reload-drift.mjs`. A full-page capture does not move anchors: rects were identical before and after on three briefs. Reloading does: in one context, loads 1–2 of the saved brief measure 15,959 CSS px and loads 3–5 measure 15,988; the tallest brief goes from 16,172 to 16,189; the rodeo checklist stays at 15,410. Height at `load` and after `document.fonts.ready` was identical in all 119 files. [measured] The cause was not established; a warming font-fallback cache is a hunch. The consequence holds either way: the engine returns anchors, links and text from the same page instance and the same layout pass as the tiles, with a layout generation number, and TD never pairs a cached anchor list with new pixels.

### The page's own notes UI

The fixed notebar (in 93 briefs) is drawn into the full-page image where it sat in the first viewport, over body text. The note-count badges are drawn in too, and would go stale the moment TD adds a note. Injecting `.notebar, .note-btn, .concur-zone { visibility: hidden !important; }` before capture keeps layout and moved no anchor in any of the 119 files (`q5-links-text.mjs`). Two briefs also have a sticky `.runbar`, whose placement in tiled captures was not established. [measured, except the last]

## 4. Modals

Method: `q4-dialogs.mjs` takes each content `<dialog>` (everything except notes.js's own `d-note` and `d-export`), clicks its `[data-dlg]` opener the way a reader would, falls back to `showModal()` if nothing opened, collects the `.notable` elements inside with rects relative to the dialog's box, and captures the dialog twice: Playwright's element screenshot and a raw DevTools clip. If anything inside scrolls, the viewport is grown until nothing does and the dialog is captured again. Two six-modal briefs: `2026-09-12-agent-native-output-surface.html` and the saved brief.

| | agent-native-output-surface | saved brief |
|---|---|---|
| Content modals | 6 | 6 |
| Openers that work | 0 (buttons present, no script wires them) | 6 |
| Open by script | 5–23 ms | 3–15 ms |
| Element screenshot (Playwright) | 301–574 ms each; all six about 2.3 s | 265–402 ms each; all six about 2.0 s |
| DevTools clip | 115–152 ms each | 128–138 ms each |
| Image size | 1408 × 1334–1926 device px | 1408 × 1366–1926 device px |
| Anchors inside | 33 (10, 3, 4, 4, 5, 7) | 13 (4, 4, 0, 4, 0, 1) |
| Modals whose body scrolls at 1400 CSS | 3 | 1 |
| Grown capture for those | viewport 1608–1874 CSS, 684–721 ms each | viewport 2288 CSS, 697 ms |

[measured] Across the corpus, 285 content modals sit in 75 briefs, and 30 of them scroll inside at a 1400-CSS-px viewport (`q5-links-text.mjs`). By grep, 6 full briefs have `data-dlg` buttons with no script referencing them. In a browser those buttons do nothing. In TD they can still work, because the button-to-dialog mapping is plain markup. [measured]

So yes: each modal renders as its own image with its own anchors. The engine opens the dialog by id (not by trusting the page's wiring), grows the viewport when the dialog's body scrolls, measures anchors on that same layout, captures, and closes it. On the DevTools path that is about 150 ms per modal, under 1 s for six, and it can run lazily on first open. [measured per modal, inferred total]

## 5. Links and text

Method: `q5-links-text.mjs`, all 119 files, one evaluate per concern. [measured]

- Links: 79 `a[href]` in the whole corpus, in 19 files (74 `http(s)`, 5 `file:`), 11 of them inside modals, at most 11 in one file. Extracting hrefs and page rects took 2.2 ms median, 5.2 ms max.
- Modal openers: 313 `[data-dlg]` buttons in 75 files. These are the clicks TD mostly has to route, to the per-modal render of section 4, rather than links.
- Text of each anchored element (for TD's note dialog): 3.4 ms median, 6.8 ms max.
- Every visible text block (outermost `h1–h5, p, li, td, th, pre, figcaption, blockquote, dt, dd, summary, label`) with its rect and text: median 97 blocks and 9,992 characters, max 280 blocks and 25,222 characters, 3.6 ms median, 6.5 ms max.

This is cheap enough to extract on every render and ship alongside the anchors: under 10 ms, and about 25 KB of text plus rects for the largest brief. [measured time and characters; the payload size is inferred] Block-level text is enough to copy a paragraph. Selecting part of a sentence would need line boxes (`Range.getClientRects`), which were not measured. [not established]

## 6. Briefs without notes.js

Method: `q6-no-notes.mjs` classifies the 26 files with no `report-notes` island, injects today's notes.js into the engine's page only (the file is untouched), and counts anchors. It then tries two ways of saving on the page with the most anchors, `2026-09-16-painting-the-outer.html`. [measured]

- 11 of the 26 are `_name_body.html` partials that an agent assembles into a brief. They are build inputs, and TD should never write to them.
- 15 are full pages (6 omit `<body>`, which HTML allows). Injected notes.js gives them 0–19 anchors: 2 pages get none, most get only their figures, and one gets 15 stat tiles and 4 figures. Injection costs 26–103 ms.
- The naive path (inject notes.js, let the page's own "save into file" run) fails. Opening the note dialog throws `TypeError: Cannot set properties of null` because the markup is missing. The download carries the injected notes.js and a new `report-concurs` island, but no notes island, so the note exists only in the `READER NOTES` comment. Reopened, the file shows no notes.
- An explicit conversion works: insert notes.css, the skill's markup, both islands, `NOTES_FILE` and notes.js before `</body>` (+30,394 bytes), let TD's writer add a note, and a fresh browser shows it with a badge.

Recommendation: show the image only, with no notes layer, for files without an island. Offer "make this brief annotatable" as a deliberate action that performs the conversion above, previews the byte count, and keeps a backup. Computing anchors TD-side for a file the browser cannot annotate would create notes that exist only in TD, a third place for notes to live. [inferred] The conversion has one lasting cost: the file's anchors come from whichever notes.js was current on the day it was converted, and an agent that later regenerates the brief will likely produce a different notes.js and different targets, orphaning those notes. [hunch]

## 7. A live engine, cheaply

### Screencast

`q7-screencast.mjs` runs `Page.startScreencast` on the saved brief at 968 × 1400 CSS, scale 1.6, with `maxWidth`/`maxHeight` set to the device size. Scroll was driven two ways: mouse-wheel events through Playwright (40 px each, about 28 a second, input-bound) and an in-page `requestAnimationFrame` loop (the ceiling). CPU is summed over the Chromium process tree from `/proc`, with 100 % meaning one core busy. [measured]

| Launch | Format | Frame | Wheel scroll | rAF scroll | Idle | PSS |
|---|---|---|---|---|---|---|
| headless defaults | JPEG q80 | 968×1400 px, 193 KB | 34 fps, 123 % CPU | 59.6 fps, 196 % | 0 frames, 4 % | 419 MiB |
| headless defaults | PNG | 968×1400 px, 304 KB | 35 fps, 136 % | 59.4 fps, 204 % | 0 frames, 3 % | 403 MiB |
| Vulkan flags | JPEG q80 | 968×1400 px, 194 KB | 35 fps, 66 % | 59.8 fps, 104 % | 0 frames, 4 % | 267 MiB |
| Vulkan flags | PNG | 968×1400 px, 304 KB | 34 fps, 74 % | 59.8 fps, 108 % | 1 frame, 5 % | 265 MiB |

Frames come out at CSS resolution, 968 × 1400 pixels, not the 1549 × 2240 a scale-1.6 display needs, so a screencast engine would show Parker's briefs at 39 % of the pixels. The Vulkan flags halve Chromium's CPU while scrolling. [measured] Whether an emulation override can make the screencast deliver device pixels was not established. On TD's side, a 968 × 1400 PNG is 39 % of the pixels of the viewport PNG that decoded in 10–20 ms, so roughly 4–8 ms a frame, a quarter to half a core at 60 fps. [inferred; JPEG decode in Rust was not measured]

### Delegating the save to the page

`q7-save-confirm.mjs` and `q7-pristine-save.mjs` add a note through the page's own UI on a copy, then click 💾. [measured]

- Playwright's `download` event fires 181 ms after the click, suggests the filename `2026-09-24-gui-in-the-tui.html` and carries the complete document: doctype, notes.js, all 8 dialogs, 137,754 bytes.
- Those bytes are a DOM re-serialisation. On three authored briefs it changed 106 lines (`attention-levels`), 295 (`bench-tenancy`) and 41 (`concur-stamp`), adding 0.1–8.9 KB: a `data-nid` on every anchor (20, 71 and 5 of them), up to 62 empty `class=""` attributes, and the note dialog's last-opened state. notes.js also writes an `id` onto every anchor that lacked one. [inferred from the code] TD's two-span write of the same notes added 0.3–0.4 KB, and the writer leaves every other byte alone (verified byte for byte on the section 2 copies).
- Each save appends a new `READER NOTES` comment and keeps the old ones: 1 comment became 2 after one save and 3 after a second. An agent grepping the file finds the stale one first.
- A note containing `</script>` breaks the file under the page's own save: the text after it leaks into the visible page, and reopened, the page shows 0 notes.

A live engine could capture that download and have TD write the bytes. It should not, for the reasons above. The notes write path belongs to TD and is the same for every engine. [inferred]

### confirm()

The ✕ button asks `confirm('Delete every note in this brief?')`. With no dialog handler, Playwright dismisses it and the notes survive. With a handler that does not answer, the page's main thread is blocked: an evaluate did not return within 2,000 ms. Accepting clears the notes and stores `{}`, and from then on a reload of the file shows 0 of its 11 baked notes. [measured] A live engine has to handle `Page.javascriptDialogOpening` itself, showing TD's own confirmation and answering with `Page.handleJavaScriptDialog`. Better still, it intercepts `[data-note-action]` clicks and routes them to TD's notes layer, so the page's storage never becomes a second copy. [inferred]

## What the engine interface needs to return

Whatever renders the page (snapshot today, Servo or a live Chromium later), TD's side consumes the same data. Absent values are absent, never zero.

- **Document identity:** path, the content hash of the bytes rendered, the layout generation, the brief's `NOTES_FILE` label, CSS width and scale, page height in CSS and device pixels.
- **Notes capability, read from the running page:** whether a notes island exists (and how many), whether a concurs island exists, whether the page's notes.js ran, whether it supports concurs (supported / not supported, never a bare false), the notes.js hash. Unknown shapes come back as unknown, and TD goes read-only on them.
- **Tiles:** given a device-pixel band, a PNG of the full layout width. A fast path gives the current viewport. The page's own notes chrome is hidden.
- **Anchors:** id, title, tag, page rect or none (inside a closed modal), owning dialog, concurrable. Same layout generation as the tiles.
- **Links and openers:** href, kind, rect, owning dialog; `[data-dlg]` buttons with the dialog id they name.
- **Dialogs:** a list of ids, then on demand a render per dialog: image, size, whether the viewport had to grow, and anchors, links and openers relative to the dialog's box.
- **Text:** per-anchor text for the note dialog; block text with rects for copy.
- **Diagnostics:** page errors, dead openers, dialogs that overflowed, layout drift between generations.
- **Not in the interface:** saving. The engine never writes the file. TD's writer does, from the bytes on disk and the anchors above. A live engine additionally takes input events and must answer JavaScript dialogs itself.

## Risks to the file

| Risk | Evidence | Guard |
|---|---|---|
| Browser localStorage hides TD's notes, and a later browser save drops them | older map and `{}` both win (measured); 22 `notes:` keys in Parker's Chromium | TD cannot fix this alone. Warn in TD when a brief's notes change. Skill-side follow-up: a revision stamp in the island that notes.js prefers when newer |
| Same-basename copies share one browser entry | `file://` origin shared across directories (measured); 8 basenames in both corpus dirs; 2 briefs point `NOTES_FILE` at another file | label the map with `NOTES_FILE`; show TD's notes as belonging to this path |
| Anchors computed by a TD reimplementation drift from the brief's notes.js | 91/93 match today's algorithm, 2 gain anchors (measured) | always run the brief's own notes.js in the engine |
| Writing against stale bytes (an agent regenerated the brief, or it was edited) | not measured | hash check at write time; re-parse the island and apply a delta; re-anchor on mismatch |
| Note text ends the script or the comment early | `</script>` leaks and loses all notes under notes.js's save (measured) | escape `</` and `<!` in islands; break `-->` and `--!>` in the mirror; both round-trip (measured) |
| Two islands, island after notes.js, island-shaped text in a comment | 1 file with two islands; 0 after notes.js; 0 decoys today (measured) | write the first; refuse if it follows notes.js; re-render the written file and verify the island and badges |
| Concurs written where the brief cannot show them | inserted island shows nothing on older notes.js (measured) | offer concurs only when the engine reports concur support |
| Brief without an island | 26 files (measured) | read-only by default; conversion only as an explicit action, with a backup |
| Writing to a build input | 11 `_name_body.html` partials (measured) | never write a file whose name starts with `_`; none of the 11 has a notes island, so the no-island rule already makes them read-only |
| Partial or torn write | btrfs, rename is atomic; no symlinks or hardlinks among briefs (measured) | temporary file in the same directory, fsync, rename; preserve the mode; refuse on a symlink |
| No undo | `~/Work/reports` (50 files) is not in git; 66 of 69 TD-repo briefs are tracked, so writes dirty that repo (measured) | TD keeps a short per-file backup ring in its state directory; say in the UI when the file is git-tracked |
| Stale `READER NOTES` comments pile up | +1 per browser save (measured) | TD replaces only the last one and leaves earlier ones alone. Collapsing them would change bytes TD does not own |
| Two copies diverge | browser saves land in `~/Downloads` (47 HTML files there), while TD writes in place | none technical; TD names the path it wrote |
| Format drift: the notes system is edited by other sessions and TD's writer falls behind | 3 releases and 7 forks in the corpus; concurs landed today (skill commit `250188f`); TD mirrors `buildMap()` and the island shapes by hand | (1) run the brief's own notes.js for anchors and capability, so tagging never needs porting; (2) a declared format version in the file, for example `data-format="2"` on the notes island, and TD refuses writes to any version it does not know and falls back to read-only; (3) shared fixtures in the skill's repository: one golden brief per notes.js release plus the forks, each with a notes-and-concurs map and the expected island, concurs and mirror bytes, run by the skill's own tests and by TD's writer tests, so a change on either side fails on both; (4) the post-write re-render check catches whatever the fixtures miss |

## Not established

- Why a brief lays out 17–29 CSS px taller from the third load in one context.
- Whether the screencast can deliver device-scale frames.
- JPEG decode cost in Rust, and whether JPEG's text edges are acceptable at scale 1.6.
- Whether text renders the same in headless Chromium as in Parker's desktop Chromium (fonts, hinting). Not compared.
- How gpui's atlas holds and releases many 12 MiB tile images. Read from code only: each image larger than 1024 px gets its own atlas texture, capped at the device limit.
- How many of the 22 `notes:` keys in Parker's Chromium are live, and what they hold.
- Where sticky elements (`.runbar`, in 2 briefs) land in tiled captures.
- Partial-sentence selection (line boxes) for copy.

## Rerunning

From this directory, with Node on PATH (the scripts load Playwright from the wellness-with-kate-site checkout):

```sh
node snapshot-engine/corpus.mjs && node snapshot-engine/versions.mjs
node snapshot-engine/q1-anchors.mjs && node snapshot-engine/q1-corpus-anchors.mjs
node snapshot-engine/q2-roundtrip.mjs && node snapshot-engine/q2-island-agreement.mjs
node snapshot-engine/q3-render.mjs && node snapshot-engine/q3-capture-cdp.mjs && node snapshot-engine/q3-resize.mjs
node snapshot-engine/q3-layout-drift.mjs && node snapshot-engine/q3-reload-drift.mjs && node snapshot-engine/q3-gpu-limits-browser.mjs
node snapshot-engine/q4-dialogs.mjs && node snapshot-engine/q5-links-text.mjs && node snapshot-engine/q6-no-notes.mjs
node snapshot-engine/q7-screencast.mjs && node snapshot-engine/q7-save-confirm.mjs && node snapshot-engine/q7-pristine-save.mjs
```

`versions.mjs` expects the skill's four notes.js releases (`250188f`, `b689671`, `eca5cb8`, `5717474`) exported first, each as `git -C ~/.claude/skills show <rev>:decision-brief/assets/notes.js > /tmp/claude-1000/td-snapshot-spike/notes-<rev>.js`, and `q3-render.mjs` must run before the Rust decode. The Rust probe builds offline against TD's pinned crates:

```sh
cd snapshot-engine/rust-probe && CARGO_TARGET_DIR=/tmp/claude-1000/td-snapshot-spike/rust-target cargo run --release --offline -- limits
```
