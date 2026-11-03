# Evidence

Depth and visibility are independent. A short claim can have a long chain of support, and the reader chooses how far to follow it.

| Setting | The links should reach | Appropriate limit |
|---|---|---|
| `sources` | The primary source for a material claim | State what was observed and what remains unverified |
| `reasoning` | Source plus the reasoning, method or measurement behind the claim | Explain inference separately from observation |
| `code-trace` | Reasoning plus relevant test, source path, symbol and repository revision | Applies where code exists; unavailable source stays unavailable |

Default to `code-trace` for code claims and the deepest available primary evidence for other work. The user can request less depth or more. Infer the setting from the request before asking about it. It is an authoring setting; a live UI control is optional, not a mandatory widget on every brief.

Use a chain the reader can inspect:

```text
brief's claim → evidence note → measurement or test → primary source
```

Each evidence note says what claim it supports, what was observed, how it was checked, what would refute it, and what remains unknown. Keep full logs and ancillary explanation there. A missing source that weakens the result still gets one plain sentence in the brief.

Prefer relative Markdown links such as `[why this holds](evidence/anchors.md#validation)`. They travel with the directory. Obsidian `[[wiki links]]` and `#^block` links are optional extensions; verify them in the chosen reader. A portable ordinary link is the default. Avoid renaming linked notes without updating their incoming links.

For code, link the path and put the symbol, revision and line in nearby prose. Use a permanent repository URL when sharing outside the local machine. Do not attach line locators to local paths when Terminal Delight is the reader. The file must remain openable even when the source has moved within it.

Give observed findings and estimates different labels. No measurement is absent, never zero. A boolean someone has not answered remains unset. A defect claim carries an invalidation check that another agent can run before fixing it.

Declare only the brief as the primary review artifact. Support can appear in an evidence list, footnote, drawer or linked note; do not register every source as a deliverable. A user who specifically requests an evidence note is making that note the review object for that turn.
