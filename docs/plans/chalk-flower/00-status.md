# Chalk — native surface status

**Difficulty: 6/10.** One existing material now has to reach every native surface and remain readable under the active palette and display settings.

**Current direction — Parker’s review on 19 October 2026:** “CHALK --- is the surface and then it FOLLOWS the TD theme.” This supersedes the first implementation’s two Chalk colour presets. His detailed screenshots authorize the continuation; [the second-pass record](02-iteration.md) holds the feedback and steps.

Chalk is one skin over the chosen TD or desktop palette. It supplies paper, chalk selection strokes and readable handwriting. The terminal keeps its font, programme colours and warp; its stock paints inside the tube. An explicit skin choice is saved independently of the theme. The old Paper id remains an alias, and the two retired design ids are migrated when reading saved choices.

## Third pass — marker placement and depth

**Difficulty: 4/10.** Parker’s next screenshots authorise centring the handwriting and marker, using contrasting pigment, and adding the tactile depth of stacked cotton paper. [The third-pass record](03-marker-and-depth.md) retains the exact feedback, references and corrective steps.

- [x] Centre marker geometry and anchor tile selection to the caption.
- [x] Fit the enlarged handwriting with compact relative leading.
- [x] Add compressed stock edges, contact shadows and fibre relief to controls and regions.
- [x] Check selection colour remains distinct from live state.
- [x] Verify the frozen build in the current saved theme, pale stock, queue, paint and CRT warp.
- [ ] Merge the revised pass and deliver the updated annotatable review.

## Second pass

- [x] Remove the two colour presets and register one Chalk surface.
- [x] Enlarge handwriting and account for it in tiles, headers and readouts.
- [x] Carry the stock through cabinet, bands, pane frame, spine/queue, palettes, gauges and paint cards.
- [x] Draw terminal stock inside the registered CRT region; keep terminal shaping intact.
- [x] Save a chosen skin with the session.
- [x] Photograph the complete native surface in Praus, light stock and a warped terminal.
- [x] Pass both terminal-core regressions, strict Clippy, formatting and release build.
- [x] Install the tested source and deliver the annotatable review page and reusable record.

## Earlier pass

The first native delivery shipped two Chalk palettes and a partial Paper skin. It passed a neutral isolated fixture but missed the inherited palette and display grade, small handwriting, selector shape and several native rendering paths Parker exposed. The original native report and evidence remain in `reports/2026-10-19-chalk-flower-native.html` and its evidence directory. The current pass replaces that product direction.

The previous GUI replacement guard rolled back on placement mismatch while retaining terminal identities. The [guarded reload follow-up](https://github.com/parker-brown-family/terminal-delight/issues/1019) remains the deployment context; installation and the running GUI must be reported separately.

**Turned out to be: 6/10.** Native checks exposed position overrides, width assumptions and texture joins that source tests could not judge. The second pass is implemented and installed for the next launch; Parker’s next visual review remains open.

**Third-pass result: 4/10.** Marker and word centring, compressed stock edges and contact shadows are implemented, natively captured and installed for the next launch. The next visual judgement remains open.

### Latest review reopened

The selection must be a single broad highlighter stroke. Subscriptions was missed by the material pass. Continuing the same approved iteration and pull request; see `03-marker-and-depth.md` and feedback-v03/14–16. Earlier “installed” and checked acceptance entries describe the superseded trial, not acceptance of these latest changes.

The revised source passes both terminal-core suites, strict Clippy, formatting and release build. The single-stroke/no-tile selector and Subscriptions material have nine final native captures. Installed build `td-chalk-single-2fc2d2d` activates on the next GUI launch; the running GUI remains the earlier integrated save-watchdog/Chalk build. Browser review and pull-request closure are the remaining checks for this revision.
