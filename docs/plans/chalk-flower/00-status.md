# Chalk — native surface status

**Difficulty: 6/10.** One existing material now has to reach every native surface and remain readable under the active palette and display settings.

**Current direction — Parker’s review on 19 October 2026:** “CHALK --- is the surface and then it FOLLOWS the TD theme.” This supersedes the first implementation’s two Chalk colour presets. His detailed screenshots authorize the continuation; [the second-pass record](02-iteration.md) holds the feedback and steps.

Chalk is one skin over the chosen TD or desktop palette. It supplies paper, chalk selection strokes and readable handwriting. The terminal keeps its font, programme colours and warp; its stock paints inside the tube. An explicit skin choice is saved independently of the theme. The old Paper id remains an alias, and the two retired design ids are migrated when reading saved choices.

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
