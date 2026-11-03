# TERMINIMALIST refinement

Difficulty: 6/10. Colour inheritance and default terminal backgrounds cross rendering boundaries; native fixtures can expose mistakes before installation.

Parker supplied eleven changes after reviewing the running skin on October 31, 2026, and explicitly authorised implementation. This continues his full-send instruction and the selected 0.5.0 integration and current-window cutover. The combined plan records those decisions; no further approval gate is pending.

## Progress

- [x] Trace the existing skin, priority markers, grade controls, note state and focus blur.
- [x] Make chrome follow palette roles; preserve the original green as Term Minimalist.
- [x] Add an optional terminal field colour control: black at zero, matching chrome at full. Existing grades and other skins retain their appearance until this control is used.
- [x] Replace fold words with plus/minus; use unboxed status symbols and priority arrows; move selection to the adjacent row bar.
- [x] Add a faint terminal boundary, Surfaces column rules and light popup blur.
- [x] Right-click marks a flat note with complementary colour and a small lift. Its existing saved attention flag continues to drive the spine pin; no physical pin is drawn.
- [x] Validate native dark/light/narrow surfaces, palette changes, note gestures, spine navigation and the slider endpoints.
- [x] Run both terminal-core suites and review the changes for integration into dev/0.5.0.
- [x] Prepare the guarded current-window cutover and updated review page. Merge and activation are recorded separately in the release receipts below.

## Decisions

The priority symbols represent a person's promoted/demoted attention setting, not navigation. They remain green up and blue down, with inherited settings dimmer.

The existing background gauge is a multiplier applied with brightness, contrast and gamma. It cannot promise an exact match to chrome at its endpoint. A separate optional field-colour channel preserves those saved settings and gives the requested black-to-match range. TERMINIMALIST defaults to matching; other skins preserve their existing field until this channel is set. Program-authored colours keep their existing handling.

The prompt boundary surrounds the terminal content region. TD does not infer the private input layout of the program inside it.

The existing FOCUS blur pass supplies popup diffusion. Use a quarter of its full radius and retain a sharp panel rectangle; routine native checks run on the isolated test display.

## Validation

Nine native captures cover Jade Horse, the saved Term Minimalist palette, Flexoki Light, the saved display grade, Surfaces, usage, gauges and 640-pixel layouts. All run on an isolated Xvfb display. The long title and simultaneous status/pin marks fit; selected-row geometry and both priority directions remain distinct.

Private input checks selected a task, folded and unfolded a project while retaining ten panes, changed the live Omarchy palette, and right-clicked a note twice. The note rose four pixels, changed to its complementary colour and returned to its original geometry and colour. The slider was dragged to 0, 50 and 100: measured field pixels were RGB (0, 0, 0), (12, 17, 15) and (20, 38, 32). At 100 the surrounding chrome measured the same (20, 38, 32). An initial sample near the field boundary was corrected against the preserved frames; the receipt retains that correction.

Default, Clean and Ops retain their furniture. Sampled navigation, frame and empty-field regions match the installed build pixel-for-pixel; dynamic RSS labels, animated logo, emitted text and random paper tilt are excluded from exact comparisons. The personal Omarchy palette generated all 19 installed application templates without unresolved fields.

Evidence is in `/home/parker/Work/reports/terminimalist/theme-feedback-proof.json`, with capture hashes, interaction receipts, template checks and the legacy comparison. Both full application suites passed. Final review further restricted row-marker sizing to open text skins and retained the legacy mirror background when field colour is unset; CI verifies the final commit with both engines. The review page is rebuilt through the annotation-preserving briefing tool.

## Release

Target: dev/0.5.0, milestone 0.5.0 — Agent-to-agent communication hardening. This is a local development update, not a tagged public release. The new personal Omarchy palette is installed for selection without switching Parker's desktop during testing.

Merge and runtime activation have independent evidence: the pull request records the integrated commit and CI, and `/home/parker/Work/reports/terminimalist/delivery-theme-feedback.json` is written after the guarded cutover. That receipt identifies the running binary, preserved process identities and retained settings. This source plan describes the implementation; it is not a claim that a particular desktop is already running it.

Post-hoc difficulty: 6/10. Palette inheritance and the optional field required consistent treatment across paint, background queries, inverse text and mirrors. Native fixtures exposed colour and geometry mistakes before the current-window cutover.
