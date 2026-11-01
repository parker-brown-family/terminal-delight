# Comment Targets

This note supports the paired engineering briefs. It describes the briefing helper itself; it makes no claim about another project's implementation.

## Mechanism

The [helper](../../scripts/brief.py) implements `resolve`. It compares a supplied document's stable identity, medium and current content fingerprint with the returned map. It checks the target against the document's anchors. Any mismatch leaves the comment in the result with `needs_review` and specific reasons.

The HTML assembler reads the canonical notes script from the sibling decision-brief directory. One guarded adaptation lets an explicit `data-brief-anchor` take precedence over a title-derived key. Renaming a heading retains its anchor. The existing format-1 notes islands are unchanged.

The shared handoff format belongs to this skill. Browser and native HTML storage remain the existing format; Markdown can use an adjacent file. Normalizing a handoff does not rewrite either reader.

## Validation

The [Python checks](../../tests/test_brief.py) exercise unchanged context, changed revision, missing target, unknown revision, wrong document, malformed islands, unknown formats, byte-preserving rebuild and broken relative links. The [isolated browser check](../../tests/browser.mjs) leaves a real comment, reloads it, exports it, and checks stable targets after a heading rename.

Run both from the skill directory. A successful test run establishes the cases it exercises; it does not establish compatibility with every reader. The session's review links its actual test results separately.

To invalidate the core claim, return a map whose document revision differs from the supplied document or whose target has been removed. If `resolve` reports that comment as resolved or drops its text, the claim fails.

## Source Trace

Open the [Python helper](../../scripts/brief.py) at `resolve`, `content_revision` and `build`. The [tests](../../tests/test_brief.py) name the expected behavior. Paths have no line suffix, so they remain openable in Terminal Delight. For a released copy, use the repository commit containing these files to pin the evidence; the local example deliberately carries no fabricated commit SHA.

## Limits

Markdown has no portable inline comment UI by itself. The adjacent map is a file the reader edits or supplies; native reader support is separate. Browser localStorage is not a disk write. HTML's old islands lack per-comment content revisions; carrying annotations through a content change produces an unknown revision in the common export. The saved backup supplies the prior context.

The existing notes engine can restore a browser-held comment another writer deleted. The assembler detects a change between its initial read and final replace, but does not provide a full multi-writer lock. Coordinate active reader writes and rebuilds. MP4 is a README earmark only.
