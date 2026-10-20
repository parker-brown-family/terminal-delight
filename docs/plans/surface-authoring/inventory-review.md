# Skin Inventory Review

The tactical workshop's inventory is a useful source map. It lists 46 rows, preserves uncertainty, and separates Palette, Design, Skin and Grade. Keep it as the starting catalogue. Its current `skin / mixed / bypass` column is too coarse to certify that a material reaches a surface: a skin radius can coexist with an unskinned background, selection and font.

This review compares the inventory introduced by `96316efd — Tactical's skin is workshopped in type and interface` with application source at `0dcdb741 — Prepare the release candidate`. It is a source audit, not a new native certification of all 46 rows. The input hash, original rows and material-check locations are in `evidence/source-audit.json`. The original document remains with the tactical workshop; this review does not overwrite that agent's work.

## Corrections

| Inventory claim | Correction and source | Consequence for authoring |
|---|---|---|
| `under_the_tree` helps draw the bay/slot | It computes a drag destination and paints nothing (`main.rs:3516`). `render_bay` and `render_bar_slot` draw those controls. The bay uses skin inks/radius; slot tracks use raw theme colours and skin radii. Neither function calls the paper material renderer. | Split the bay and allowance rails. Mark their material coverage as absent at this snapshot, even where colour or geometry follows the skin. |
| Handover chip: skin | `pane/handed.rs:387` uses semantic skin inks, radius and hover, then constructs its own face, border and literal type size. It does not use a material-bearing chip constructor. | Classify ink/corners as routed and material as bypass. |
| FOCUS: bypass is deliberate to preserve monospace | The content uses measured snapshot font/cell geometry (`main.rs:32818`, `32838`). The shell independently uses literal corners and shadows in `reader_panel` (`17234`); its inherit toggle also bypasses the skin (`17937`). | Preserve the measured content. Route the reader's frame and controls separately; the font constraint does not justify exempting the whole reader. |
| Vitals location: unknown | The agent-wall `bar_row` closure draws them at `main.rs:29765`. Its track/corners use the skin radius, while its fill encodes semantic charge and unknown readings. | Add a named child entry under agent cards. Preserve its data encoding while skinning the track/material. |
| Agent cards: bypass | Their render already uses skin radii and optical text sizing, including the vitals rows (`main.rs:29799`, `29819`). That is partial adoption; the card material is still independent. | Use facet coverage rather than a whole-card bypass label. |
| Sticky colours come from `notepaper` | That is the base path. Paper also overrides Contrast note colours in `pane.rs:11453–11461`, and passes a material-specific flag to `sticky::paint` (`sticky.rs:869`). | Record the override. Keep note colour choice and note decoration as explicit decisions in a custom look. |
| Document body font: inherited, unknown | A floating document hosts a `DocumentView` (`pane/doc.rs:330`). That view dispatches HTML, Markdown, image, video and PDF backends (`docview.rs:17`). HTML/PDF/media content owns its typography or pixels. TD's strip and notes controls are separate native UI. | Split frame, native controls and hosted content. Do not apply the management font to an authored page or a video. |
| Tooltips: font unknown | `SlotTooltip` and `ThemeTooltip` use skin type sizes and inks, but their shared shell does not explicitly set a family (`main.rs:25843–25948`). | Keep family inheritance unverified until a native font probe. The source does not warrant a claim that every tooltip uses the skin family. |

The inventory's `hacker.toml` curvature question is also resolved at source: the legacy key is present, but `FileEffects` has no such field and the application uses `Grade::warp`. Do not use that key in an authoring example. This is a configuration observation, not evidence of a new rendering failure.

## Expand These Families

The 46 rows group several independently drawn parts. Retain the top-level map and enumerate these children before using it as a completion checklist:

- **Workbench:** the existing `surface::Kind` variants, plus composer, menus, rail and unknown-kind presentation. A corner test in `benchdraw.rs:4688` establishes corner routing only; it does not establish material or font coverage.
- **Documents:** native notes toolbar, draft/save/discard dialog and attachment controls (`docview/notes_ui.rs`); reading progress rail (`docview/progress.rs:132`); video transport (`docview/video.rs:433`) and PDF status controls (`docview/pdf.rs:810`). The page, picture or film inside them remains content.
- **Subscriptions:** launch-agent configuration (`main.rs:9297`), provider/view choices, limit tracks, pace track and refresh. They share a shell but have separate drawing paths.
- **Agent cards:** card shell, actions, preview, vitals and unknown readings. The miniature terminal preview keeps its terminal renderer.
- **Popovers:** retain window and pane contexts separately. They can share a visual recipe without sharing the input/hit-testing implementation.

## What Each Entry Needs

Use one canonical catalogue, exported to the human inventory and consumed by the skill. An entry identifies an owner and records independent facets: ground/material, boundary, emphasis, typography, semantic colour policy and content boundary. It also names a native fixture and carries an optional proof receipt. An unrun fixture has no receipt; it is not a passing scene.

Source checks can reject a missing owner symbol, an unregistered fixture and a new direct material branch. They cannot discover every new inline renderer or certify a screenshot's feel. Review the changed render paths and run the native atlas as well. A manual catalogue without those checks will age like this Markdown table.

The detailed row table in the visual brief preserves the original 46-row map and attaches these amendments. No coverage percentage is reported: the rows have different granularity, and this audit did not recapture every native state.
