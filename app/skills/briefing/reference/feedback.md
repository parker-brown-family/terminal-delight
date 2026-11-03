# Feedback

The shared contract is about a reader's reaction to a slice of a particular brief. Each medium keeps its own way to select and store that slice.

| Field | Meaning |
|---|---|
| `format` | Version of this handoff envelope; currently 1. Distinct from HTML island format 1. |
| `document.id` | Stable brief identity; HTML and Markdown renditions get distinct identities. |
| `document.href` | A locator supplied with the map. Resolve identity and revision against the actual document rather than trusting a basename. |
| `document.medium` | `markdown` or `html` now. Video is reserved. |
| `document.revision` | Content fingerprint the comments refer to, or `null` when it cannot be established. This is distinct from an annotation save timestamp. |
| `comments[].target` | Stable element or block identifier within the document. |
| `title` | Readable target context, or `null` when the exporter has none. |
| `text` | The reader's words, preserved exactly. Empty is valid only with an explicit stance. |
| `stance` | `agree`, `disagree`, or `null` for no stated stance. No comment or unchecked control implies no opinion. |
| `at` | Comment timestamp if known; otherwise `null`. |

See the [editable Markdown example](../examples/engineering.feedback.json). Keep the map beside its document. Supply both to the agent, or copy the map while naming the original document. Map export is not document publication or approval by itself.

Markdown hashes the exact UTF-8 document bytes, so even a rewording flags the old map for review. HTML hashes the authored content region using canonical parser events; transient annotation attributes, the generated section rail and reader-controlled evidence/disclosure state are excluded. Equivalent browser serialization of void elements and implicit table bodies keeps the fingerprint. Renaming an HTML heading changes the revision while its explicit target remains stable. A manual content edit is detected when the helper recomputes that fingerprint.

HTML continues to store native notes in `report-notes`, explicit concurs in `report-concurs`, and the reader-notes mirror. Never invent a content revision for a legacy map. If it is unknown, verify the targets and context with the reader's document before acting.

## Copy Map Text

In a standalone browser, the reader's notes live in browser storage, and the **Copy map** button is how they leave. The text it copies is what usually arrives in chat:

```text
NOTES — cache-eviction-8dade01d6271
1 notes on 1 elements · 1 concur.
Each [anchor] is an element id in that file — search it to find the passage.

[sec-return] 02 The Return  ✓ concur
  · Show what happens when this section is renamed.
```

`resolve` accepts that text in place of a JSON envelope, from a file or from stdin (`-`), and normalizes it at the handoff without changing the reader's storage. Its grammar, line by line:

| Line | Shape | Read as |
|---|---|---|
| Header, required | `NOTES — <brief id>-<12 hex>` | the brief's id, and the first twelve hex digits after `sha256:` of the page `revision` the reader had open (the same value `check` prints) |
| Count and hint | `N notes on M elements…`, `Each [anchor] is…` | ignored; optional |
| Target | `[<anchor>] <title>`, optionally ending `  ✓ concur` | the target and its readable title; `✓ concur` is an explicit agreement |
| Note | two spaces, `· `, the reader's words | one comment on the last target |
| Blank | empty | ends the target's block |
| Unknown revision | `REVISION UNKNOWN — …` | the bridge's mark for notes carried from earlier wording |
| Removed target | `[<anchor>] UNRESOLVED — target missing`, then note lines; a note reading `Explicit agreement on the earlier target` is an agreement | comments whose passage is gone |

Text before the header is ignored, so a reader's greeting can stay. Windows line endings, a leading byte-order mark and a chat quote (`> ` on every line) are accepted. The twelve digits go into `document.revision_prefix`, and `document.revision` stays `null`, because that field holds a full fingerprint or nothing; `resolve` compares the prefix with the current page, unless a `REVISION UNKNOWN` line makes the revision unknown.

The envelope says `"source": "copy-map"` and `"text_exact": false`, because the copy joins each note's lines with spaces. Quote the reader's words from it, but treat line breaks as lost. Text with no `NOTES —` line is refused rather than guessed at.

To export embedded HTML feedback or a new blank Markdown map:

```bash
python3 scripts/brief.py feedback path/to/brief.html
python3 scripts/brief.py feedback path/to/brief.md
```

To resolve a saved envelope:

```bash
python3 scripts/brief.py resolve path/to/brief.md path/to/brief.feedback.json
```

To resolve Copy map text the reader pasted:

```bash
python3 scripts/brief.py resolve path/to/brief.html - < pasted-notes.txt
```

The helper adds `current_revision`, and each comment gets `status` and `reasons`. A changed revision, unknown revision, different document or missing target produces `needs_review`; the text stays in the output. Exit 2 is a request to inspect those comments, not an empty map. Unsupported format or malformed data produces exit 1 and a visible error.

Resolve feedback before revising. For each comment, answer it, change the brief, or state why it remains open. A resolved target means the comment's location is established; it does not mean the concern is answered. Keep that distinction in the agent's response. A concur applies to the recommendation the reader saw, not automatically to a later recommendation.

HTML's existing native islands lack per-comment content revisions. After a content-changing rebuild carrying old notes, their revision becomes unknown in the exported envelope. The old file is retained as a content-addressed backup. Review old and current context rather than claiming a precise mixed-origin history. A host holding unsaved browser notes must export or save them before regeneration.
