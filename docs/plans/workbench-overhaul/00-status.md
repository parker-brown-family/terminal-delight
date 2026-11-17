# Workbench overhaul

Difficulty 8/10. A compact card can expose an approval while hiding the evidence needed to judge it.

Product, Architecture, Program design and Vertical slices were approved by the release orchestrator on 2026-11-17. [Product note](01-product.md) · [Approved mockups](mockups/cards.html) · [Photographic audit](00-audit.md)

## Plan

1. Make the decision question and its option costs readable at 600 px. The window test opens a real decision and checks the painted text and actions at both widths.
2. Keep the typed data model in `surface`, drawing in `benchdraw`, and interaction in `pane/bench`. Give each kind a readable body, move shared actions near the title, and give the Overview a compact list. Check all kinds, the fallback and twenty replies in the window.
3. Route keyboard focus through the controls the pane actually draws. Exercise evidence, targeted review actions and scroll from the window. Reverse each fix to check its test.
4. Register Blueprint as a separate built-in skin. Verify its palette, borders, grid and type with contrast calculations and private-window captures of Settings, the Floor, help and the workbench.

## Calls

Use the existing decision contract. Approve and Reject apply to the proposal. A recommended option stays a recommendation. Keep the question, each case and each cost at both widths.

Use one body at both widths for prose, decisions, patches, diagrams and unknown kinds. At narrow widths only tables need a different arrangement. Repeat the field labels in each record.

Left and Right move among visible controls. Enter uses the same target as a pointer press. Keep the surface identity with a comment draft so that a replacement card cannot receive it. Keep Up and Down for traversing the conversation. Let the shelf scroll independently and keep the selected row visible.

Give the current and selected reply a preview and use the title for older replies. Keep the window-wide attention rail and the per-pane Overview scoped as before.

At wide widths, put decision options side by side and wrap architecture groups. At narrow widths, keep them stacked. The window test measures both option labels to distinguish those layouts.

Blueprint uses static dimension lines and the direct terminal renderer. Check the hover face and the intersection of two grid strokes when choosing annotation ink. Keep the saved skin ID independent of its translated name.
