# Painter palette

Difficulty: 5/10 — an existing appearance surface, with saved state and an agent-status dependency to preserve.

The combined plan was approved with seven notes on revision 2. Revision 3 received an explicit concur on Accepted amendments: Theme, Style, Rules; glass areas and house switches; no explanations under the main headings; Gamba removed; 800ms initial hover and 180ms nearby. Theme hovers contain only the uppercase name and palette.

Implementation is complete on the palette branch. Terminal State is integrated from main; its catalog, hover description, handler and saved `agent_tint` key are preserved. Its row uses the same slider switch and timing as the other rules.

Validation: both complete core suites passed (2,299 default and 2,252 fallback app tests, plus integration suites). The final lazy hover preview passed the default app suite again and strict Clippy on both cores. Formatting, 78 kiosk checks, focused retirement checks at 968/390px and the 31-page docs build passed. The full website verifier exceeded a five-minute harness limit without a result; focused checks cover the affected theme chips, removed route and generated theme page. The approved HTML passed hover timing, all descriptions, both scopes, note export/save and narrative checks.

Native rollout uses an immutable release, the hidden old-host compatibility smoke, and the guarded GUI-only reconnect. Installation evidence is reported in the local HTML delivery; the host and terminal processes must remain running. Layout sizing is checked against the actual option inventory; the HTML is an illustration, not a native screenshot.

[Differentiate Design and Skin](https://github.com/parker-brown-family/terminal-delight/issues/990) carries the DIN 1451 direction for Field and Tactical. DIN is not installed on this machine; font provisioning and the stronger visual distinction remain that issue's scope.

Turned out to be: 5/10. The state dependency hidden inside the removed animation needed care; the appearance handlers and persistence could otherwise be reused.
