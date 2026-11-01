# Markdown

Write the same short explanation you would put on the HTML page. Use headings and plain paragraphs, with evidence behind links. A diagram is useful when the chosen reader can render it reliably; an HTML illustration is often the better medium when the picture is essential to understanding.

`scripts/brief.py new name.md --id STABLE-ID --purpose PURPOSE` writes the identity comment, two anchored sections and `TODO(brief)` where the writing goes; `check` refuses until every one is replaced. By hand, add one source identity comment:

```markdown
<!-- briefing: {"id":"request-cache-summary","medium":"markdown","purpose":"summary"} -->
```

Give important sections stable explicit anchors. HTML anchor tags work in readers that allow inline HTML; Obsidian block identifiers are another option where supported. Keep the identifier after a heading rename.

```markdown
<a id="sec-result"></a>
## Result

The cache now keeps entries for five minutes. [Evidence](evidence/cache.md#validation)
```

Do not make that example claim about a real project without evidence. See the [working engineering example](../examples/engineering.md) for this skill's own mechanism.

Use ordinary relative Markdown links by default. Obsidian-style navigation means small supporting notes linked to one another; it does not require every link to use wiki syntax. Check any optional wiki or block syntax in the target reader.

A reader with native section commenting may store and send its own note map. Test section selection, reload persistence and export before promising that interaction. A plain Markdown file alone supplies no inline comment editor.

The portable fallback is a saved adjacent `name.feedback.json` file. Generate its document identity and current revision with `scripts/brief.py feedback name.md`, then add comments to the `comments` array. Each comment names a section or block target and carries the reader's words. The map is inspectable text, persists as an ordinary file, and can be copied into the next prompt or supplied with the document.

Run `scripts/brief.py resolve name.md name.feedback.json` before acting. A changed document, absent revision or missing target remains visible. Preserve the map when revising the document; never regenerate it over a reader's comments. Export a new blank map only to a new file or after explicitly retaining the prior one.

The adjacent map is a fallback, not a claim of visual parity with HTML. State which interaction was verified. Declare only the Markdown brief as the review artifact; keep the map and its supporting evidence out of the primary artifact surface.
