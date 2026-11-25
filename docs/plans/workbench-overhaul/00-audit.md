# Workbench before the overhaul

At 600 pane pixels, a reader can approve a decision without seeing its question, or accept a change without seeing its patch. The same payload in a 1,600-pixel pane shows both. The default and TERMINIMALIST skins share the behavior.

## Ranked findings

1. **Review actions survive the loss of review evidence.** The narrow decision lists two options. The question, argument, cost and consequences appear only in the wide card. A narrow changeset shows two file names and line counts above six action buttons. The wide version includes both diffs.
2. **The keyboard reaches only the first action.** The reading handler maps Up and Down to a card and Tab to the next shelf. Enter takes the first declared action. A keyboard user cannot move that focus from Approve to Reject, pick a particular patch target, or switch the response from Reading to Evidence. The changeset's first action requires a hunk target. The keyboard path supplies none. These are findings from the [key handler](../../../app/src/pane/bench.rs) and [action definitions](../../../app/src/surface.rs).
3. **A narrow card can discard the relationships that explain its contents.** The table removes the column headers. The architecture lists five node names under a count of five edges, without showing which nodes connect. The Markdown preview stops after the Review heading, above the list the reader needs. The remaining vertical space is visible in each photograph.
4. **The wide body spends most of its width on short lines and framing.** Each option and file gets an almost full-width band. In the response, introductory metadata and two rows of reading controls precede a paragraph with a long line length. With twenty replies the rail repeats a preview under every headline. In the narrow default photograph fourteen titles are visible, while the body beside them holds just one short answer.
5. **Action emphasis changes with the furniture.** Default draws the primary verb as a filled button and secondary verbs as outlined chips. TERMINIMALIST writes the verbs as words, with a small leading accent on the primary. The response also uses words for tabs and registers. A visible keyboard target would help distinguish the control being operated in both skins.

## What appears on each card

| Kind | First reading at 600 px | First reading at 1,600 px |
| --- | --- | --- |
| Response | Title, provenance, two rows of reading choices, plain answer. The rail repeats a clipped opening. | Estimated effort and confidence appear before the reading choices. The answer spans the card. Evidence is a small tab above the paragraph. |
| Decision | Title, option count, two option names and Approve. The recommendation is a small mark. | Estimates precede the question. The first option has a recommendation, case and cost. Consequences and actions sit below the second option. |
| Changeset | Two identical file paths with different counts. The separate action row distinguishes hunks by suffix. | File, verdict and colored patch lines identify what changed. The action row is detached from the patches it affects. |
| Architecture | Five bullet labels. The subtitle counts edges, but none are drawn here. | Groups and states appear in rows with a separate list of connections underneath. Short labels occupy narrow strips of very wide panels. |
| Artifact | A description and wrapped path above Open and Comment. | A field list adds MIME and finding. It repeats the description from the heading. A rail click opens the document immediately. These pictures show the underlying card after closing the document. |
| Table | Four rows shown as prose joined by dots. A dash carries the missing cell. The heading says there are four columns but never names them. | Four headings align the cells. The missing cell explicitly says unavailable. Open space separates a short boolean from the final descriptive column. |
| Markdown | The title and provenance lead into styled headings and a paragraph. The preview ends at the next heading. | The same document shows its list and final sentence. A line about undeclared estimates comes before the document. |
| Unclassified | The unknown kind's reason appears twice. The original payload is inaccessible at this width. | The unknown kind and raw JSON both survive. The provenance identifies the file drop's writer as unknown. |

TERMINIMALIST replaces shelf and action chips with text, the rail handle with a word, and prominent boxes with rules. The content differences between widths persist in both skins. The slightly taller narrow TERMINIMALIST changeset has three rows of action words. Neither version offers a keyboard focus indicator for those verbs.

## Photographs

Each link opens the original window capture. **Feed** is a single agent's twenty replies. It checks history density on the per-pane Overview. An operator moving among twenty live agents was unmeasured.

| Kind | Default 600 | Default 1,600 | TERMINIMALIST 600 | TERMINIMALIST 1,600 |
| --- | --- | --- | --- | --- |
| response | [View](screens/before/default-600-response.webp) | [View](screens/before/default-1600-response.webp) | [View](screens/before/terminimalist-600-response.webp) | [View](screens/before/terminimalist-1600-response.webp) |
| decision | [View](screens/before/default-600-decision.webp) | [View](screens/before/default-1600-decision.webp) | [View](screens/before/terminimalist-600-decision.webp) | [View](screens/before/terminimalist-1600-decision.webp) |
| changeset | [View](screens/before/default-600-changeset.webp) | [View](screens/before/default-1600-changeset.webp) | [View](screens/before/terminimalist-600-changeset.webp) | [View](screens/before/terminimalist-1600-changeset.webp) |
| architecture | [View](screens/before/default-600-architecture.webp) | [View](screens/before/default-1600-architecture.webp) | [View](screens/before/terminimalist-600-architecture.webp) | [View](screens/before/terminimalist-1600-architecture.webp) |
| artifact | [View](screens/before/default-600-artifact.webp) | [View](screens/before/default-1600-artifact.webp) | [View](screens/before/terminimalist-600-artifact.webp) | [View](screens/before/terminimalist-1600-artifact.webp) |
| table | [View](screens/before/default-600-table.webp) | [View](screens/before/default-1600-table.webp) | [View](screens/before/terminimalist-600-table.webp) | [View](screens/before/terminimalist-1600-table.webp) |
| markdown | [View](screens/before/default-600-markdown.webp) | [View](screens/before/default-1600-markdown.webp) | [View](screens/before/terminimalist-600-markdown.webp) | [View](screens/before/terminimalist-1600-markdown.webp) |
| unclassified | [View](screens/before/default-600-unclassified.webp) | [View](screens/before/default-1600-unclassified.webp) | [View](screens/before/terminimalist-600-unclassified.webp) | [View](screens/before/terminimalist-1600-unclassified.webp) |
| feed | [View](screens/before/default-600-feed.webp) | [View](screens/before/default-1600-feed.webp) | [View](screens/before/terminimalist-600-feed.webp) | [View](screens/before/terminimalist-1600-feed.webp) |

## Capture method

The baseline executable reported clean commit `a47eabf8`, debug profile, optimization level 0. A private Xvfb server on display 97 ran one window against isolated state, config, runtime directory and session host. The demo supplied the cards through the file transport. The rig added a Markdown document, pointed the artifact at a private Markdown file, and added nineteen named response fixtures for the feed photographs.

The layout's own width measurement determined when to capture. Default needed windows 888 and 1,888 pixels wide. TERMINIMALIST needed 656 and 1,656. Each window was 950 pixels tall. In both skins the rail measured 180 pixels at the narrow size and 208 at the wide size. The rig selected each card through the window's hit zones, verified the displayed surface ID and captured the X11 window as WebP. Counts and dimensions describe this fixture and configuration. No timing benchmark was run.
