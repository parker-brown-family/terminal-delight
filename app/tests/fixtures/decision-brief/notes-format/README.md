# Notes format fixtures

A decision brief keeps its reader's notes inside its own HTML file, and more than one
program writes them there: the brief's own `notes.js`, when the reader clicks
💾 save into file, and Terminal Delight, which edits a brief's notes in place. These
fixtures hold every writer to the same bytes. They change in the same commit as any
change to `assets/notes.js`, `assets/notes.css` or `reference/notes-markup.html`, and
Terminal Delight vendors this directory byte for byte, with a `SOURCE` file naming the
agent-skills commit it copied.

## Format 1, in bytes

`writer.mjs` is the reference writer and the rule; this is the summary.

- **Two islands.** The first `<script … id="report-notes">` holds
  `{ "<anchor>": [{ "text", "title", "ts" }, …] }` and the first `id="report-concurs"`
  holds `{ "<anchor>": "<ts>" }`. `ts` is UTC to the minute, `YYYY-MM-DD HH:MM`. A note's
  other fields ride along untouched. Found by the open-tag pattern
  `<script\b[^>]*\bid\s*=\s*["']?report-notes["']?[^>]*>`, case-insensitive; the text runs
  to the next case-insensitive `</script`. Text is read as `JSON.parse(text.trim() || '{}')`.
- **The version.** `data-format="1"` on the notes island, set by the markup block and by
  every save. No attribute means a brief from before format 1: a writer keeps it in that
  shape (no attributes added), escaped and with one mirror. Any other value is refused.
- **The revision.** A format-1 write sets `data-format="1"` and then
  `data-rev="<ISO 8601 UTC, to the millisecond>"`, `new Date().toISOString()`, on each
  island it writes, the way `setAttribute` serialises: an existing value replaced in place
  and double-quoted, a new attribute last.
- **The JSON.** `JSON.stringify(map, null, 1)`, then every `</` written `<\/` and every
  `<!` written `\u003c!`. Keys keep the order the file had them in.
- **The concurs island** is rewritten when the brief's notes.js supports concurs. When
  none exists, one is inserted straight after the notes island's `</script>`, and only if
  there are concurs to put in it.
- **The mirror.** `"<!--\nREADER NOTES —\n" + map + "\n-->"`, where `map` is notes.js's
  `buildMap()` with `--!>` made `--! >` and then `-->` made `-- >`. It replaces the last
  such comment that follows the notes island, else goes before the last `</body>`, else
  at the end of the file. Its header names the page's `NOTES_FILE`, not the path.
- **Everything else** is carried over byte for byte, and writing the result back with no
  edits changes nothing.

A writer refuses, and writes nothing, on: `NoIsland` (no notes island: the page is
read-only, not empty), `IslandAfterScript` (notes.js reads the island before it is
parsed, so a browser could never show it), `Unreadable` (the island is not a map of the
right shape), `UnknownFormat`, `BuildInput` (a file named `_…`), `AnchorGone` (a note or
concur on an anchor the page does not have), `ConcursUnsupported` (a concur on a brief
whose notes.js predates them), and `UnterminatedMirror`.

## What a browser shows (notes.js, format 1)

The islands are what an agent reads; localStorage keeps the reader's edits between
reloads. notes.js also keeps, under `notes-sync:<NOTES_FILE>`, the revision it last took
in and every note and concur it has taken from an island or written into one. On load,
for notes and concurs alike: nothing stored shows the island; the island's revision
being the one last taken in shows the stored copy; anything else, including an island
with no revision, shows the stored copy plus every note and concur in the island this
browser has not seen. It never drops what it holds, and what the reader deleted stays
deleted. So a note deleted from the file by another writer survives in a browser that
still holds it.

## A case

```
cases/<case>/brief.html        the input bytes
cases/<case>/edits.json        { "rev": the revision the write stamps,
                                 "edits": [{ "op": "add" | "delete" | "concur" | "unconcur",
                                             "nid", "title"?, "text"?, "ts"? }] }
                               a delete finds its note by text (and ts, when given)
cases/<case>/anchors.json      [{ nid, title, concurrable, stamp: { r, dx, dy } | null }],
                               document order, from the brief's own notes.js in Chromium
cases/<case>/expected.html     the bytes after the write; absent when it must be refused
cases/<case>/expected-map.txt  what buildMap() returns once the written file is reopened
cases/<case>/expect.json       the page's facts before the write (notes_file,
                               concur_support, format, rev_before, notes_text as the
                               browser reads it, null when there is no island), refuse,
                               what a fresh browser shows before and after, and which
                               regions the write touched
```

`null` in `expect.json` means the thing does not exist on that page (no island, no
concur counter), never zero.

| Case | Pins |
|---|---|
| `current-pristine` | The ordinary path on this release: empty islands declaring format 1, two notes, two concurs; the first save stamps both islands and inserts the mirror before `</body>` |
| `concur-era-pristine` | A brief from 250188f, the concur release, with no format declared: written in that shape, with no attributes |
| `b689671-saved`, `eca5cb8`, `5717474` | A brief from each release before concurs, already saved once by its own notes.js; the write replaces its mirror and never adds a concurs island |
| `hostile-text` | `</script>`, `</SCRIPT >`, `<!--`, `<!--<script>`, `<![CDATA[`, `-->`, `--!>`, `---`, non-ASCII, emoji, newlines, and from another writer a CR, a control character and U+2028: every one round-trips exactly |
| `orphan-note` | A note whose anchor is gone is counted in the map header and never listed; keys out of document order and an unknown field survive |
| `two-islands` | The markup pasted twice: only the first island of each kind is read and written |
| `stale-mirrors` | Three `READER NOTES` comments: only the last is rewritten |
| `notes-file-override` | `NOTES_FILE` names another file: the map header and the download use it |
| `no-body-close` | No `</body>`, and a notes island with no text: the mirror goes at the end of the file |
| `no-island` | No notes system at all: read-only, refused `NoIsland` |
| `island-after-script` | Refused `IslandAfterScript` |
| `unreadable-island` | Refused `Unreadable`; the page shows nothing and throws nothing |
| `future-format` | `data-format="2"`: refused `UnknownFormat` |

TD's program design named the first thirteen. `current-pristine` and `future-format`
arrived with format 1.

## Running

Needs Node, a Chromium, and playwright-core 1.45 or later. `browser.mjs` finds
playwright-core through normal resolution, else through `PLAYWRIGHT_FROM`, a directory
whose `node_modules` holds it; Chromium is `CHROMIUM`, else `/usr/bin/chromium` or
Google Chrome.

```sh
node decision-brief/fixtures/notes-format/check.mjs
node decision-brief/tests/notes-js.test.mjs
```

`check.mjs` checks every case four ways: the page's anchors and facts, the writer's
bytes (and that nothing outside its three regions moved), the written file reopened
fresh, and the page's own save made through its buttons, whose downloaded islands and
mirror must equal the writer's. The page's save re-serialises the rest of the document,
so the rest is not compared. `tests/notes-js.test.mjs` drives notes.js through the
ordinary path and through issues #24 and #25.

Both take `--notes-js <commit>` to inline another release's notes.js and show what it
gets wrong; with `250188f`, every behaviour test and the current release's page-save
checks fail.

After changing notes.js, notes.css or the markup block, rebuild, which re-derives every
expected file, then check:

```sh
node decision-brief/fixtures/notes-format/build.mjs
```

Every brief is synthetic, built from this skill's own markup at the release named in
`expect.json`, so no real brief's content lands in two public repositories.
