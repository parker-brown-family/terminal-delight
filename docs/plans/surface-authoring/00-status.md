# Custom surface authoring

**Difficulty: 6/10. Post-hoc: 7/10.** Native integration and concurrent mainline changes cost more than expected. Repeated release builds and expanding fixture coverage made the turn too long; the next run needs one bounded proof matrix and one final build.

The combined plan was approved on 2026-10-20 through all three concurs and Parker’s “Basically concur all”. All four implementation slices are complete:

- [x] Subscriptions tracer and native dark/pale fixtures.
- [x] Shared material/type policy, bounded data and legacy compatibility.
- [x] Canonical catalogue and exported native atlas runner.
- [x] Second file-only look, reload/persistence/restore proof and published portable skill.

The sibling’s OPS vocabulary and subsequent mainline fixes through 50cf0106 are incorporated. The primary checkout and normal running windows were preserved. The separate installed screenshot-paste build is not replaced by this work.

## Review and proof

Open `reports/2026-10-20-custom-skins-built.html` from the repository root. It contains native captures, the implemented architecture, all 65 catalogue entries, proof limits and the public skill link. The original annotated plan retains its three concurs.

The full default suite passed 2,570 tests and the fallback suite passed 2,523, each with ten ignored. Later boundary changes passed 36 skin tests and 24 allowance tests plus both CI Clippy variants. Final action-order and capture-isolation changes passed strict Clippy and release builds. GitHub CI passed at the preceding integrated head; the final review push starts a fresh CI run.

There are 29 final native captures across 25 registered scenes. All account-collector canaries remained untouched; the workbench sample ran for ten seconds. Two skins passed valid edit, rejected invalid edit, exact restoration and saved-selection persistence in private previews. These are scoped static reviews: hover, drag, full keyboard interaction and seven catalogue families without dedicated fixtures remain unverified.

Browser checks exercise desktop/mobile fitting, all inventory note anchors, image zoom, notes export/save/clear, external-request absence and JavaScript errors. The narrative diagnostic classified the implementation page as a journey; it is not evidence of universal cold-reader comprehension.

## Deliverables

- Implementation: https://github.com/parker-brown-family/terminal-delight/pull/1035
- Published skill: https://github.com/parker-brown-family/agent-skills/blob/main/td-skin/SKILL.md
- Guide: `docs/skins/README.md`.
- File-only example: `docs/skins/examples/field-notes.toml`.
- Native proof: `docs/plans/surface-authoring/evidence/native/`.
- Feedback and failures: `docs/plans/surface-authoring/implementation-log.md` and `evidence/native-failures/`.
