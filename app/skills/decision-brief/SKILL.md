---
name: decision-brief
description: The Workbench edition of the decision brief, shipped inside Terminal Delight. Build one self-contained HTML page, drawn rather than written, when your work is something a person has to read and then decide on — an audit, a review, an options comparison, a findings write-up, a proposal. Present it on your bench as an artifact; the person reads it in a square inside the Terminal Delight window, notes any element, and the notes come back into your prompt as an anchored map. Five minutes to read, a figure for every argument.
---

# Decision brief — the Workbench edition

A brief exists to get a decision made by someone with less time than the material
deserves. In the Workbench that someone reads it in a square floating over your own
pane, between one prompt and the next. **Budget five minutes.** Everything below
serves that number.

This is Terminal Delight's edition of the `decision-brief` skill. The assets,
references and `brief-wall` beside this file are the skill's own, byte for byte
(`SOURCE` names the commit). This page is shorter, and stricter about length,
than the one they came with. Where the two disagree, this page wins.

## When to build one

Build a brief when the person has to **see** something to decide: a surface, a
mechanism, several findings weighed against each other, options with costs.
Do not build one for a question with two or three short answers — that is a
`decision` surface — or for a change to review, which is a `changeset`. A brief
per turn is noise. One brief per decision is the rate.

## The loop

```
you build the brief    →  present it on your bench as an artifact
the person clicks it   →  the brief floats over the bench, inside the window
they note any element  →  a stamp of CONCUR on a decision is one click
↪ send to agent        →  the notes map lands in your prompt, not sent
they press Enter       →  you read the map and act on it
```

Nothing in that loop leaves the window. The notes are saved into the HTML file
itself, so the brief is still the record tomorrow.

## Five minutes, drawn

These numbers are the edition. The upstream skill budgets twenty minutes; the
square is narrower than a browser tab and the reader is mid-conversation.

| Part | Limit |
|---|---|
| Headline | One sentence, the verdict, at most forty words, in the one `.glass` card |
| Read-first | At most **three** items, each three lines: what · why it matters · what it takes |
| Figures | **One per argument.** A section with no figure is a caption waiting for a picture, or it is cut |
| Paragraphs on the page | At most three sentences. Anything longer goes in a `<dialog>` modal with a one-line entry point |
| The page | About three screens of the square. Depth lives in modals, where nobody is forced through it |
| Decisions | A grill: each question with your recommendation attached, so a concur answers it |

The test: a reader who looks only at the figures and their captions comes away
with the argument. If they would not, draw more and write less. When you are
unsure whether to draw one more figure, draw it.

## Four invariants

**1 · Notes on every element.** Inline `assets/notes.js`, `assets/notes.css` and
the markup block in `reference/notes-markup.html`. No external `src`: the brief
must survive being mailed as one file. `notes.js` tags the elements and gives each
a stable anchor. Set `window.NOTES_FILE` to the file's own name.

> **The inlining trap.** A `<script>` ends at the first literal closing script tag,
> even inside a JS comment or string. If one lands in the JS you inline, the rest
> of the script becomes page text and notes never run, with no console error.
> Check `document.querySelectorAll('.notable').length` after assembling; zero
> means you hit it.

Leave the `#report-notes` and `#report-concurs` islands in, with their
`data-format="1"`. Terminal Delight writes notes into them in place, and so does
the brief's own save button. Never hand-edit those islands when you revise the
brief.

**2 · The budget drives the structure.** Verdict first, then read-first, then the
figures, then the grill. A reader who stops anywhere has enough to act at that
depth.

**3 · Every claim carries its confidence.** `measured` · `inferred` · `hunch` ·
`contradicted`, as `.tag` classes. A defect also carries the check that would
prove it wrong (`reference/evidence.md`).

**4 · The brief is drawn.** Hand-built HTML, CSS and inline SVG: no library, no
image files, nothing fetched. A **mockup** wherever the reader judges a surface,
drawn at the width they would read it, with the real strings. A **diagram**
wherever they judge a mechanism. Every picture is a numbered `<figure>` with a
`.lbl` (`03 · What opening costs`) and a caption that makes a claim.
**Read `reference/pictures.md` before writing the body**; its repertoire of
eleven figures is where to start, and the pictures decide the body.

## The look

Every brief is built over a random Omarchy wallpaper, in that theme's colours,
with the glass of Terminal Delight's docs site. `reference/glass.md` says what
makes it work: depth from layers, numbers that glow, one milky `.glass` card,
restraint. Four blocks go in `<head>`, in this order:

1. `assets/base.css`, inline.
2. The `<style id="brief-wall">` block, from one run per brief:
   ```bash
   python3 <this directory>/scripts/brief-wall --out /tmp/brief-wall.html
   ```
   Take the random pick; never choose a theme to match the subject. It needs
   `vips` and Omarchy's themes. If it exits with an error, leave the block out:
   `base.css` falls back to a plain glow, and a brief without its wallpaper is
   still a brief.
3. The brief's own `<style>`.
4. `assets/notes.css`.

Use what `base.css` already styles instead of restyling it: the masthead
(`header.top`, `.eyebrow`, `h1`, `.lede`, `.stamps`), `h2 .n`, `.esc-list`, `.tiles`,
`.cards` with one `.card.rec`, `.finding`, `.grill .ask` with `.rec`, `figure`,
`.pair`, `pre`, `table`, `.callout`, `.tag`, `.linkbtn` and `dialog`. The theme
owns the ground, the ink and the accent (`--bg`, `--fg`, `--dim`, `--acc`); the
status colours are fixed, so red always means a decision is waiting and green the
recommended path.

## Put it on the bench

Write it to `reports/<YYYY-MM-DD>-<topic-slug>.html` in the project you are working
in. Never a bare date: several agents may write briefs in one project on one day.

Then present it as an `artifact` surface, with an absolute path:

```json
{ "td": "0.4", "kind": "artifact", "id": "brief-cache-eviction",
  "title": "Cache eviction: LRU, with one guard",
  "model": { "href": "/abs/path/reports/2026-09-25-cache-eviction.html",
             "mime": "text/html",
             "summary": "Three findings, two options, one decision waiting" },
  "weight": { "effort": "medium", "confidence": "measured" } }
```

A click on that card opens the brief in the floating square over the bench. If
you are connected to Terminal Delight's MCP server, you may also call
`open_document` on it (placement `beside`) so it is already open when the person
looks up. Your `response` for the turn says in plain words what the brief decides
and that it is on the bench. Do not paste the brief's contents into the reply.

## Read the notes back

The person presses **↪ send to agent** in the square's notes bar, or
Ctrl+Shift+Enter over the square. The map lands in your prompt — or, when your
pane is showing its bench, in the bench's composer — **without Enter**, so it
arrives when they choose to send it. It looks like this:

```
NOTES — 2026-09-25-cache-eviction.html
1 note on 1 element · 1 concur.
Each [anchor] is an element id in that file — search it to find the passage.

[fig-03-what-a-miss-costs] 03 · What a miss costs
  · Draw the cold start too, not only the steady state.

[ask-evict-by-recency-or] Evict by recency or by age?  ✓ concur
```

Each `[anchor]` is an element id in the file: find the passage by its id rather
than rereading the page. A concur with no note means *approved as recommended*.
The `document_notes` MCP verb answers the same map on request, unsaved notes
included.

When you revise a brief after notes, keep the anchors stable: the same figure
numbers, the same grill questions. A note whose anchor disappears is kept, but it
no longer points at anything.

## Verify before you present

Check the DOM, not a screenshot. If you can drive a headless browser, serve the
file over `http://127.0.0.1:<port>` and run:

```js
// horizontal overflow, the most common silent break
document.documentElement.scrollWidth > document.documentElement.clientWidth
// every modal trigger resolves
[...document.querySelectorAll('[data-dlg]')].filter(b => !document.getElementById(b.dataset.dlg))
// every svg scales and is described: [viewBox, no fixed width, aria-label]
[...document.querySelectorAll('svg:not([aria-hidden="true"])')].map(s =>
  [!!s.getAttribute('viewBox'), !s.getAttribute('width'), !!s.getAttribute('aria-label')])
// notes coverage, never a bare count: [selector, present, tagged]
['.grill .ask', 'figure', '.card', '.finding', '.tile', '.callout'].map(s => [s,
  document.querySelectorAll(s).length,
  [...document.querySelectorAll(s)].filter(e => e.classList.contains('notable')).length])
```

A grill question that cannot take a note has failed at the only job the document
has, so read that row first. Without a browser, at least confirm the file opens in
the square and the notes bar counts the elements.

## Traps

- **No theme toggle, no template with slots, no prescribed sections.** The shape
  comes from the material.
- **Do not explain a title with a subtitle.** "Tokens", not "Tokens — what they are
  and why one of our rules is wrong".
- **Do not announce honesty** ("stated honestly"). Use a confidence label.
- **No superlatives** about things you have not exhaustively tested.
- **A long section on the page is competing with the brief.** Move it to a modal.
- **Never let a bare identifier carry a sentence.** Say the thing; put the issue
  number or commit where it can be clicked.

## The reference files

`reference/pictures.md` (read first), `reference/glass.md`, `reference/layout.md`,
`reference/evidence.md` and `reference/notes-markup.html` are the skill's own.
They were written for the person who commissioned the format and call the reader
"Parker": read that as the person at the window. A path under `/home/parker` in
them names his exemplar and exists only on his machine. Where they budget twenty
minutes or five read-first items, this page's numbers apply.
