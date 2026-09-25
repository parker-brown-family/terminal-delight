# Article profile — Terminal Delight docs

Read by the `article` skill. It carries the craft; this file carries what is
specific to this site. Drafted 2026-09-24 from Parker's direction in the session
that started the site; the intake answers marked *(default)* were chosen by the
agent and are his to overturn.

**Site:** https://docs.terminal-delight.brownfamilysports.com — static, served by
Caddy on piper-prod from `/var/www/td-docs`, DNS through Cloudflare. Publish with
`docsite/deploy.sh`; a git push does not deploy.
**Renders from:** `docsite/build.mjs`, no dependencies. `nav.json` is the page
list, `layout.html` the only copy of the shell, `pages/<slug>.html` one content
file per page, and `assets/td-docs.css` the only place page components are
styled. A page goes live the moment its content file exists; until then the
spine shows it as "soon".

## Writing a page

```html
<script type="application/json" data-page>
{ "title": "Keys", "description": "…the search snippet, carrying the answer…",
  "updated": "2026-09-25", "trueOf": "main at d7cf383",
  "summary": "a few words for the page map" }
</script>

<section data-register="brief">
<p class="lede">…</p>
<p class="invite">Story … Technical …</p>
</section>
<section data-register="story" data-headline="The doorway"> … </section>
<section data-register="technical" data-headline="Keys"> … </section>

<ol data-sources>
  <li id="s1">…</li>
</ol>
```

- Registers come in the order Brief, Story, Technical, and only Brief is
  required. The Brief has no headline, because the title is its headline. A
  page with one register gets no tab bar.
- Every h2 and h3 gets an id (`technical-keys`) and a `#` link, and search
  indexes the page heading by heading. Link across pages as `/slug#id`.
- Cite with `<sup><a href="#s1">1</a></sup>`. The build fails on a citation
  with no source.
- Commands a reader will paste go in `<pre class="cmd"><code>…</code></pre>`,
  which gets its own Copy button. Key tables take `class="keys"`.
- Never style in a content file. If a page needs a component, it goes in
  `assets/td-docs.css`, where every page can use it.
- The build refuses: a broken internal link or anchor, a /docsite/ path, an
  unfilled token, and the phrases in Parker's writing ledger (the banned
  vocabulary, the split negation, a labelled declarative opener, a heading
  ending in a full stop, a label written as a question). It warns when a
  Brief falls outside 60–260 words or has no closing invitation.

## Audience

- **Publishes:** how Terminal Delight works, one feature per page.
- **Read by:** people running coding agents in Terminal Delight, people deciding
  whether to, and agents that get a page pasted into them (hence the copy
  header).
- **Reader's relationship to the author:** user of a tool. *(default)*

## Registers

- **Axis:** treatment. Keys `brief` / `story` / `technical`, labels **Brief**,
  **Story**, **Technical** — the same axis and labels as parkerbrown-dev, which
  is where Parker pointed.
- **Default register:** Brief.
- **Brief** is answer-first: what the feature is, what you do with it, the one
  key to remember. 120–200 words, then one line of invitation.
- **Story** is the analogy (Parker: *"STORY is the analogy"*). One metaphor, held
  to the end; scene, question, turn, resolution; ends with the reader holding the
  real terms. 500–900 words. Optional — a forced analogy is worse than none.
- **Technical** is the reference: tables of keys, strings exactly as the UI
  prints them, file paths, protocol fields, limits, and a closing paragraph on
  what would prove the page wrong. No cap. Every number resolves to a source.

## Voice

- **Person:** second person, present tense — *you flip the pane*. *(default)*
  Docs describe a tool the reader is holding; they are not a build log.
- **Real strings only.** A label, key or path is quoted as the code defines it,
  and checked against `main` at the commit named in the page's meta row.
- **Byline:** none on the page. The copy header names the source.
- **Never:** marketing adjectives, a feature described by what it is not, or a
  sentence about the implementation where the reader needed the behaviour.

## Page furniture

- Meta row under the title: `Updated <date> · true of main at <sha>`. A docs page
  read cold must say which build it describes.
- Figures are hand-built inline SVG on the shell's tokens, so they repaint in
  glass and paper. Captions make a claim.
- CTA: `Install` in the top bar opens the Install page. Parker: *"it is poor
  form to trap someone without their reading first"*, so nothing on the site
  downloads before its page has been read. No conversion ask anywhere else.

## Liability gate

- **Required:** no.

## Categories

The spine groups (Get started, Agents, The bench, Workspace, The look, MCP and
scripting, Desktop, Reference) are navigation, not categories. No chip renders.
