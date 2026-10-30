# Relationship Builder

**Difficulty: 6/10** — selection, drag routing and one-to-many saves share a surface; a wrong target changes another agent's access. One combined plan and one approval.
**Turned out to be: 6/10.** Native events caught a small-window layout regression and a pool membership shortcut that bypassed staging. The single review also changed the picker into the actual Floor before implementation.

- Combined plan: **APPROVED with amendments 2026-10-28**. Parker: “remarks, increased scope. Let's keep going!”
- Review page: `reports/2026-10-28-relationship-builder.html`.
- Integration branch: `dev/0.5.0`; intended release: 0.5.0.
- Worktree: `/home/parker/Work/td-floor-relationship-builder`.

## Slices

- [x] Wire Connect, card-on-card drop and Alt-drag into one actor/verb/targets builder; prove selection and cancellation with native events.
- [x] Add target cards, multi-target validation, per-pair updates and receipt invalidation; prove the whole selection is applied once or validation changes nothing.
- [x] Apply active skin materials and translations; verify at narrow and wide sizes, run both terminal-core checks, and prepare a reviewable development build.

## Resume

Read `plan.md` and the notes/concur islands in the review page. The page is a design prototype, not the native implementation. No application code changed during planning. The user's final direction is an on-page builder, visually distinct through the existing active glass material; it replaces the earlier popup proposal. The selected card is the actor. Alt+left drag draws a line; ordinary card drop on a different card opens the same builder. Multiple targets are part of this task.

The daily checkout is on an unrelated usage branch; do not change it. This worktree starts from `origin/dev/0.5.0` at a6dffed0. The older `/home/parker/Work/td-dev-0.5.0` checkout is stale and lacks Threshing Floor code. Inspect current refs rather than trusting its files.

## Prototype verification

The browser prototype passes empty and selected entry, self exclusion, three-target preview, actor replacement and absent-verb checks. All three figures and both decision rows have note coverage. No page overflow was observed at actual viewport widths of 520 and 1573 pixels. A QA annotation survived reload; browser automation then timed out during cleanup. Parker's later saved remarks and concur are preserved in the prototype. Structural validation confirms unique IDs and no external assets. Those checks preceded the native implementation below. Evidence: `verification.json`; narrative result: `narrative.json` (journey, opening 0.72).

## Approved amendments

The saved remarks in the HTML supersede the separate picker in the prototype. The actual chart supplies target cards. Keep the active candidate relationship surface permanently reserved so opening it never shifts the chart, pool, filter or controls. The existing filter stays active and editable while composing. Pool cards may be staged in the draft; submitting adds those cards to the org and saves the relationships together. Cancellation adds nothing. Group related controls and introduce whitespace around those groups. The one-verb/all-targets decision carries Parker's concur. Continue implementation without another plan approval.

## Native result

Implemented in code commit `0159d220c7fd659e551adde8fc44fd8b7b4f5fe2`. The fixed composer uses the actual chart and pane pool; no extra picker appears. Filter input remains active. Alt-drag draws a tether. Staged pool candidates are weak references and gain membership only when the validated batch is submitted. Actor and selected targets are highlighted. Target summaries keep uncomputed values unknown. Missing selections retain their cached names for errors. A seat recorded in multiple orgs is rejected as ambiguous membership; submission preserves those records and saves no lines.

Both full suites pass: 3,000 tests on the default core, 2,947 on Alacritty, 17 existing ignored per run. The final native view pass has 44 passing tests. CI-style Clippy commands pass on both cores, formatting passes and the release binary is built. An optional all-targets Clippy experiment reported pre-existing lints in unchanged test code; the repository CI commands do not enable that mode.

The staged binary and digest are in `build-receipt.json`; the Ops-skin native capture is `evidence/native-ops.png`. Human review: `reports/2026-10-28-relationship-builder-built.html`. The running daily window was not restarted.

The final report renders in headless Chromium, has three annotatable figures and six annotatable behavior rows, and uses no external assets. `native-verification.json` records the structural checks. The narrative gate returned journey, with opening 0.30; the optional build provenance still scored as requiring project context. The page links the full-size native capture and keeps source details folded away.
