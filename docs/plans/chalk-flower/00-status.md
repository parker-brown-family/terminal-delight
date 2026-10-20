# Chalk Flower Native Style and Skin

**Difficulty: 6/10.** Control geometry, texture and UI typography need native visual and performance checks.

**Approval:** Parker requested the pictured concept built end to end on 19 October 2026, including a distinct sticky-note treatment. That instruction authorizes implementation of the combined plan below.

The style supplies two palettes, warm gesso and slate. Both request the paper skin and retain JetBrains Mono for terminal cells. The skin owns UI handwriting, paper controls and pigment texture. An explicit skin choice continues to override a style's preference.

The renderer reuses one cached grain image. Paper edges are deterministic and have bounded detail. Terminal shaping, input, screen effects and host protocol stay on their existing paths. Sticky notes retain their current placement, curl, editing and inverse warp; their paper gains fibres, tape and a flower mark.

## Slices

- [x] Register the two styles and paper skin; separate UI typography from the terminal font.
- [x] Draw stable paper controls and cached pigment grain; add sticky-note details.
- [x] Verify all skins, both terminal cores, native controls, restart persistence and rendering cost.
- [x] Deliver a page with native captures.
- [ ] Install the verified build.

**Turned out to be: 6/10.** Palette cross-checks caught small-print contrast and status-colour collisions. Native tests measured the rendering cost before delivery.
