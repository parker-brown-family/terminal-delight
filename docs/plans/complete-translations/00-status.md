# Complete the App Translations

Difficulty: 6/10. Most work is reversible wording, but the human/agent boundary and narrow native controls need source review and rendered checks.

## Selected Scope

Parker authorized the supplied workflow on 2026-11-04: build the batch inserter, classify readers, translate grouped surfaces through browser ChatGPT Pro, wire and validate all nine languages, photograph German and Japanese, and repair the detector. The native app is in scope; the documentation website is separate. Translation wording is delegated under `docs/TRANSLATING.md`.

Release intent is **0.5.2**, GitHub milestone 4. The integration destination is **dev/0.5.1**. Work started from `ea393457`; the current opacity-gauges changes were integrated at `361d0644` before final validation. The original dirty checkout remains untouched.

## Progress

- The standard-library inserter validates all languages, English, placeholder/keycap multisets, plural shapes and explicit cognates before an atomic write. It verifies installed tables after formatting.
- The complete matrix contains **179 human-facing fields**, **182 text rows**, and all **nine languages**. Source and returned batches remain under `batches/`; `complete.json` and `translations.csv` are the installed subset.
- The inventory classifies **237 detector candidates** and **97 shared icon bindings**, including mixed access prompts, archive diagnostics, model states, update states and copied agent text.
- Model and effort ids retain their wire values. Typed captions and catalog states translate at rendering time. Archive records and OS diagnostics retain their original data.
- The shared skin icon converter now carries typed language-pack caption functions. This closes the path that native screenshots exposed after ordinary `.control(...)` calls were translated.
- Help footers interpolate the actual translated tab labels. Physical keys remain canonical throughout the packs.
- The detector recognizes text controls, status literals and bezel helpers across Rust formatting. It excludes standalone tests, element ids and fill-key noise. CI verifies the inserter and installed matrix.
- Final gates, native proof and pull-request metadata are recorded in `validation.json` when complete.

## Reader Decisions

Three detailed host warnings were excluded after their callers proved to be engine/CLI code. Their translated source batch remains as provenance; no unused UI fields were installed. CONFIG keeps its established menu name and existing cognate policy.

The old engineering-state issue permits invalidation when no English engineering line is drawn for people. That is now the case: the human panel uses translated coordination data, while `Reading::report` remains the English MCP report. No speculative typed-readout refactor is needed.

The historical 39-label width issue distinguishes a soft estimate from an observed defect. The current batch's width exceptions are explicit in `width-report.json`; native scene receipts establish the states actually photographed. Unopened slots remain unverified.

## Validation

```bash
python3 scripts/test_i18n_batch.py
python3 scripts/test_translation_suggest.py
bash scripts/test-translation-suggest.sh
python3 scripts/i18n-batch.py docs/plans/complete-translations/complete.json --verify
cargo fmt --manifest-path app/Cargo.toml -- --check
cargo clippy --manifest-path app/Cargo.toml --locked -- -D warnings
cargo test --manifest-path app/Cargo.toml --locked --no-fail-fast
```

One existing font-wiring source guard initially failed because it assumed the control call stayed on one line. It now finds the same glyph literal independently of that formatting and preserves the check that its font is explicitly selected. The next full suite passed before the final converter change; the final source then passed 3,340 Rust tests across ten targets (18 default-ignored tests), ten Python tests, historical detector fixtures, formatting, strict clippy and the build. Native proof covers 28 capture cases, including narrow windows and the repaired painter labels.

## Browser Repair

The October browser component could create tabs but omitted the network policy required by the current Chrome extension when attaching. An official signature-verified replacement restored navigation, DOM, Pro selection, uploads, downloads and screenshots. The private system incident records the component backup and verification; its package-management follow-up remains outside the app change.

Post-hoc difficulty: 6/10. Native verification caught the most consequential omission: icon-derived words bypassed the first literal inventory.
