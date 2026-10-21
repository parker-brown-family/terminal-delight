# Four independent text-linger controls

Difficulty: **5/10**. Timing and settings are local to the existing effect; the risk is coupling controls or changing legacy explicit zero. Parker's continuing end-to-end instruction authorizes implementation, testing, integration into `dev/0.5.0` and the daily build. The prior lighting promotion has merged and is running; this is the next requested change.

Parker: “Text linger needs to be a slider --- what we have should be the MAX -- slide down to 0 - 2 sliders for on cursor, off cursor, THEN for active / inactive terminals.”

## Selected behavior

Put two millisecond sliders in each of ACTIVE TERMINALS and INACTIVE TERMINALS. Cursor linger ranges from 0 to 220ms; off-cursor linger from 0 to 650ms. Default each to its current maximum. Zero disables that category, and the old erased-text minimum must not override the selected shorter time. New text still appears immediately. Keep the approved glow/text levels.

The requested words may refer either to the current short text-change/long erased-text trails or to actual geometric cursor position. An optional clarification is pending while independent settings/UI work proceeds. Preserve the current two trail categories unless Parker selects geometric classification; that preserves the approved appearance at the maximums.

## Implementation boundary

Keep four optional timing values and independent named grade channels. Retain `afterglow_ms` as a legacy setting/API field, so old saved grades and explicit zero still have meaning. Resolve absent new values from the legacy setting only at drawing time; changing a new active dial must not alter an undeclared inactive dial. Preserve old ownership pins when an outer scope adopts the new fields. Store no invented zero for an undeclared control.

Reuse the retained glyph classifier, row batching and independent live-text cache. Pass the selected focus pair to history. Changing one timing retimes or removes only its category's retained letters, keeping their original birth time. Keep the visible glyph bound and the production GPU patch stack unchanged.

Remove the single AFTERGLOW slider section. Add the pair below letter glow in both terminal sections, use milliseconds in the live value readout, and translate the labels/helper in all nine language packs. MCP reports and patches use explicit milliseconds with the same independent bounds.

## Verification and delivery

Test all four ranges, zero, intermediate lifetimes and exact expiry; normal/erased independence; focus separation; absence versus saved zero; pin merge/reset and old-config migration; serialization and MCP validation/readback. Keep the existing 100 skin/design/glass/crawl cases, Unicode grouping and live-layout cache tests. Run both complete core suites and strict lints, then native gauge/zero/intermediate checks. Open one new PR for this post-promotion request against `dev/0.5.0`, with the 0.5.0 milestone. Preserve the local palette/note preview when building the integrated daily snapshot. Verify the exact running binary and all open shell identities, archive the full layout and notes, restore any transient floating document and distinguish explicitly closed holds from open panes.

## Implementation and verification

All four optional grade fields, named ownership channels, gauges and nine-language labels are implemented. The runtime selects the focused pair and independently retimes each retained category without renewing its birth. Zero removes only its category. Both disabled controls take the existing cheap clear/return path. The old unified field remains an API compatibility reset; explicit new fields in the same patch win. A legacy timing pin expands to all four fallback channels, and gauge reset also clears and owns the hidden legacy fallback.

The native capture confirms both pairs at 220/650ms, four distinct intermediate values and saved zero after restart. The optional wording clarification received no answer; this implementation preserves the approved text-change and erased-passage classifier, with that meaning stated in the translated helper and MCP description. It does not add geometric cursor-distance classification.

Eight new tests cover independent clamping, exact 1/125/325/650ms erasure expiry, category suppression, original birth preservation, MCP omission/readback/reset ordering and legacy pin migration. Real pane tests cover focus routing and a slider change while both trails are present. Existing coverage still draws all 100 built-in skin/design/glass/crawl combinations. Complete suites pass 2,640 default and 2,587 fallback tests, with 10 ignored in each. Native proof and crops are in `reports/linger-sliders-proof/`.

An extra all-targets lint run found existing test-only lint debt as well as one new test initializer warning. The initializer is corrected; the exact unchanged base reproduces ten older warnings. The repository's required production lint commands are evaluated separately; the existing test-lint follow-ups remain the place for broader cleanup.

Turned out to be **5/10**. The ownership fallback and retained timing changes needed care; the existing renderer and geometry tests covered the surface without a new GPU effect.
