# Chalk surface — second native pass

**Difficulty: 6/10.** This corrects the material boundary across existing native surfaces.

**Direction confirmed by Parker, 19 October 2026:** “CHALK --- is the surface and then it FOLLOWS the TD theme.” The ten review images and the detailed corrections authorize this continuation of the existing workshop. Colour choice remains with TD; Chalk owns paper, chalk strokes and handwriting.

## What the first pass missed

The first pass added two colour presets and a paper skin. Native captures proved those presets in an isolated neutral fixture, but did not exercise the inherited desktop palette, custom display grade, full menu, outer cabinet, attention queue and gauges Parker actually uses. Passing the fixture was insufficient evidence of a complete surface.

Parker’s feedback: “the font is awkwardly small on the menu”; “the selector ... is trash - needs to be the chalk stroke”; “Teh  theem surface was INCOMPLETE!”; “sliders etc ALL need to have the SURFACE respect the paper surface”; “The terminals themselves are unchanged (this may have to do with the CRT warp render colliding...)”. He called it “a really good first shot” and requested: “TRACK THE STEPS YOU TOOK AND THE FEEDBACK - we will be doing this AGAIN! AND AGAIN!”

## Shape

One Chalk skin follows any active TD or desktop palette. This material correction is separate from the ongoing [Field/Tactical DIN design differentiation](https://github.com/parker-brown-family/terminal-delight/issues/990). Remove the two Chalk design choices. Keep old Paper selection compatible. Skin owns a readable UI type scale, paper regions and controls, chalk selection stroke, and terminal stock. The material is drawn inside the terminal tube before text so CRT curvature carries it with the terminal. The terminal keeps its current font and program colours.

## Steps and acceptance

- [x] Correct the picker and compatibility for saved first-pass choices.
- [x] Apply the handwriting size and chalk stroke through controls and the left tree.
- [x] Complete the outer cabinet, top/bottom bands, attention spine and queue.
- [x] Complete palette/gauge panels, switches, tracks and thumbs.
- [x] Draw terminal stock inside the registered tube, before the cached grid.
- [x] Capture the same native surfaces with light and dark desktop palettes, flat and warped.
- [x] Run relevant regressions, build and install; record what actually runs.

## Repeatable review

Start each pass from the reader’s screenshots. Map every observation to a surface and a rendering owner. Patch the shared primitive first, then its bypasses. Photograph each listed surface in the user’s palette and grade; compare the photograph to the feedback, rather than accepting a test count as visual proof. Record the change and whether each observation was resolved. Keep earlier captures and decisions; append the next review instead of replacing the history.

## Step log

1. Retrieved the first implementation, native evidence and live-review feedback. Confirmed the gap is material coverage and palette independence. No terminal or session host is restarted during source work.

2. Removed the two design presets, kept the old Paper id as a compatibility alias, and added optical UI sizing. Shared paper stock now reaches the frame, bands, spine/queue, palette/gauge panels and switches/sliders. Terminal stock paints inside the registered tube before the cached grid.
3. The first regression run caught stale constructor anchors in existing source tests and an accent/selection collision in a new assertion. Preserved the existing distinct selection role; changed only its drawn stroke. Updated source-test anchors to tolerate formatter line breaks.
4. Added session persistence for the independently chosen skin; the former fixture had restored a supplied skin file rather than proving a picker choice survived. The second fixture restores Chalk from ordinary session state.

5. Native Praus capture exposed a new layout bug: `Skin::finish` set `relative`, overriding an already absolute menu. Preserved the caller’s positioning, enlarged note tiles to fit handwriting, and added a pure position regression. Also guarded ordinary skins against receiving paper backgrounds on formerly transparent bands. These were caught before installation.

6. The light native gauge capture caught wrapped percent values and low-contrast tracks. Gave readouts their own non-wrapping width, enlarged the panel, and separated the dark track/thumb ink from the coloured chalk fill. Pale-stock selection pigment is darkened from the same palette role.
7. Larger handwriting exposed the queue header’s width assumption. The keyboard hints move below its heading, and the long queue scrolls within the available window height. The pane header’s overflow thresholds now account for the optical type scale.
8. Audited the remaining independent renderers: the native pane frame, raised CRT bezel and paint cards. Chalk replaces the hard bezel with a paper rim while the terminal’s warp continues to carry the stock and text. The FOCUS transcript keeps its measured monospace size.
9. Background command deduplication reused an in-flight build while inputs changed. Final verification therefore uses fresh direct build processes, a manifest of changed app sources frozen after formatting, and a comparison before installation. Freeze a capture runner before starting it; do not rewrite a shell script while it is executing.

10. Full native paint cards exposed long palette names exceeding their old tile width; widened Chalk tiles and enlarged their key hint. Pale stock also exposed joins between repeated grain tiles at 1.6× display scaling. Each region now samples one shared continuous sheet image, reducing image submissions and removing the internal joins.

11. The fixture helper cleans up its saved session after exit. The evidence assembler initially could not recover the captured profile; the runner now copies its seed and restored state before cleanup. A final frozen runner produced all nine native scenes from the same verified binary.

12. The browser check verifies eight figures, two choices, responsive widths, zoom and note add/export/save/clear. The new report has its own annotation filename and preserves explicit figure anchors. The archived first report still uses a placeholder filename; its safe annotation migration is recorded in the [archived-review follow-up](https://github.com/parker-brown-family/terminal-delight/issues/1021). Existing notes were not touched.

13. Froze the source hashes after formatting. Fresh processes passed 2,494 Rio tests and 2,447 Alacritty tests (nine ignored in each), strict Clippy and the release build. Native Default switching and isolated session restart passed. The commentable page, capture recipe, palette profiles, logs and hashes are retained under `reports/2026-10-19-chalk-surface-v02-evidence/`.

14. Installed the verified binary as `td-chalk-surface-c8e4715` under the shared deployment lock. That suffix is a binary hash, not a commit id. The launcher points to it for the next launch; the current GUI and session host were left running. Reopen the GUI, then choose Skin → Chalk. Source hashes were compared again immediately before installation.

15. The review page’s first prose-only narrative measurement classified its sections as a catalogue. Revised the joins to follow the actual dependency: palette boundary, surrounding controls, terminal warp, then the next review. The retained final measurement classifies it as a journey; it remains a diagnostic for this visual review, not a substitute for the native captures.

**Turned out to be: 6/10.** The shared material was straightforward; independent renderers and optical text sizing required the native feedback loop. Source tests alone would not have caught the visual failures above. The next pass starts from annotations and the same surface checklist.
