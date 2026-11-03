# Verification

Check what a reader will rely on. A large passing count is not evidence that every significant target is commentable.

| Check | Required observation |
|---|---|
| Purpose and medium | The source names both; the purpose answers the reader's actual need. |
| Brief | The main explanation is readable without opening the evidence. Material uncertainty remains visible. No `TODO(brief)` remains. |
| Evidence | Local links and fragments resolve. A code claim reaches a real symbol/test/revision, or says the missing part is unavailable. |
| Feedback | A comment names its intended target and survives the verified persistence route. Export retains its words and location. Pasted Copy map text resolves against the page it came from. |
| Revision | Renaming a heading preserves its explicit target but flags the old content revision. Missing targets and unknown revisions remain unresolved. |
| Rebuild | Embedded HTML notes, concurs and mirror survive byte for byte. Malformed or unsupported islands refuse before replacement. |
| Artifact boundary | Only the brief is declared primary. Supporting evidence and maps do not become competing deliverables. |
| HTML | Glass gate passes; headings and figures have controls; dialogs work; text fits figures; no page overflow at pane and phone widths. |
| Markdown | Inline features are tested in the actual reader or clearly identified as unavailable; the fallback map is usable without that reader. |
| Decisions | Every `.ask`, `.grill-q` and `.q` in a grill carries an anchor, so each takes a note and a CONCUR; `check` refuses one that does not. |
| Calls | Every open or asked call in the named ledger appears as a row; each row's evidence is the bytes the call recorded; `calls` exits 0 only when every call is answered and its evidence unchanged. |
| Orient | Approved, unstamped and overturned decisions match the stamps and the ledger; a missing parent prints as unavailable; a looping derivation is read once. |
| MP4 | Visible future earmark; no implemented annotation or playback claim. |

Run both suites from the skill directory. They work in temporary directories and change nothing in the skill. Add a path to put your own HTML brief through the browser checks too:

```bash
python3 -m unittest discover -s tests -v
node tests/browser.mjs
node tests/browser.mjs path/to/your-brief.html
```

The Python tests exercise preservation, stale feedback, identity mismatch, unknowns, missing targets, reader controls and literal markup in comments. They also cover the guards and what a clean-context agent found: a rebuild keeping the real wallpaper, and repairing a wallpaper an earlier rebuild stripped; the page's file mode; Copy map text with Windows line endings, a byte-order mark or a chat quote; every broken link reported at once; sources that name themselves; links with titles, images, reference definitions and `file://` URLs; code examples that are not links; slots inside the content region; a hand-edited page; an unrelated file at the output path; missing sibling skills; a failed wallpaper and `--no-wall`; scaffolds that refuse until written; and Copy map text from a file, from stdin, from another revision and with removed targets.

The browser check uses its own headless Chromium and temporary profile. It checks exact target coverage, heading rename persistence, a real comment's reload and export, narrow layouts and the evidence selector. It pipes the page's real Copy map text through `resolve`, then does the same after the commented section is removed and expects exit 2. It never sends input to the user's desktop. It exits 77 when Node 22+ or a Chromium-family browser is missing; report that as not checked.

Run the sibling HTML look gate too. Until its wallpaper check is fixed (agent-skills#52 — the glass gate passes a page with no wallpaper), it also passes a page built with `--no-wall`, so confirm the wallpaper by eye or by `doctor`. The narrative gate can judge the telling of a substantial review page with an explicit intended reader. It cannot establish the truth of its findings.

If verification cannot run, state the missing check and the reason. Do not replace “not checked” with “passed” or a zero-failure count.
