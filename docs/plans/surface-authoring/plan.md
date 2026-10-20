# Custom Surfaces

**Difficulty: 6/10.** Strengthen the approved skin seam, make its coverage inspectable, and prove that another look can be authored through data. This is one combined plan and one approval before implementation.

## What It Is For

Someone should be able to show an agent a reference and receive a personal TD skin without repeating today's tour through native renderers. Existing palette recipes, custom-file loading, hot reload and read-back already support that workflow. Chalk added a cached material renderer, but adopting it still required material-aware branches in application code. The proposed skill depends on closing that gap.

The source audit and original catalogue are explained in `inventory-review.md`. The reader-facing decision page is `reports/2026-10-20-surface-authoring.html`.

## The Shape

Keep the existing separation of palette, design, grade and skin. Preserve the current skin file, explicit selection, saved ids and last-valid-file reload behaviour. Strengthen the existing semantic element vocabulary instead of replacing GPUI or inventing a second theme system.

| Boundary | Responsibility | Existing code to retain |
|---|---|---|
| Skin specification and resolution | Optional recipes, metrics, material and type settings baked against the active palette | `SkinSpec`, `Skin::bake`, `SkinRegistry`, headless `skin` command |
| Shared visual parts | Region, action, choice, state badge, meter and caption layout; background layers precede text | Existing `panel`, `bar`, `chip`, `btn`, `active_row`, `paper_option` and `paper_caption` |
| Material renderer | Grain, silhouette, stock edge, lift and marker painting | Existing cached `paper.rs` implementation behind a private dispatch boundary |
| Typography | Family, optical size, leading and fitting boxes, independent of material | Existing `workbench::Step` / `Type` ramp and `Skin::with_type`; preserve separate chrome and reading gauges |
| Catalogue and proof | Owners, per-facet coverage, content exceptions, fixture names and optional receipts | Incoming skin inventory and existing native demo/capture hooks |

Callers describe purpose and state. They do not ask `is_paper()` or import the paper renderer. Existing wrapper names can delegate during migration. A first implementation should add only the missing semantic parts; it should not rename every helper for uniformity.

Illustrative interfaces, to be resolved with the tracer slice:

```rust
// Proposed additions to the existing Skin vocabulary, not a new UI runtime.
fn region(&self, kind: RegionKind) -> Div;
fn choice(&self, state: ChoiceState) -> Div;
fn caption(&self, role: TextRole, step: Step, text: &str) -> Div;
fn meter(&self, reading: Option<f32>, meaning: MeterMeaning) -> Div;
```

`RegionKind` selects an existing panel treatment; `ChoiceState` preserves selected, unset and disabled distinctions needed by the caller. Text roles distinguish management labels, numbers and code. The existing type ramp supplies sizes. A meter's unknown reading stays absent and draws its existing unknown state. These signatures are a design sketch; they are not implemented APIs.

Make existing renderer variations authorable as optional data: optical type scale/minimum/leading, marker emphasis, grain strength, edge roughness and lift. Defaults reproduce the current shipped skins. Keep current `[font] family` compatible. Typography must stop depending on `Material::Paper`: choosing a monospace UI on paper must not inherit Caveat's 1.45 optical correction.

A file may compose supported strategies without a rebuild. A reference that requires a new silhouette or rendering effect needs one shared engine extension. The skill must expose that boundary rather than claiming an arbitrary new renderer can be described by today's TOML. Local texture assets and font provisioning need explicit validation if introduced; arbitrary executable plugins are outside this plan.

## The Catalogue

Keep one machine-readable catalogue in the TD repository. It records stable ids, owner symbols, scopes, visual facets, semantic-colour/content policies and named native fixtures. Export its human table and JSON from that source. The public `td-skin` skill reads the export for the installed/check-out version rather than maintaining a copied inventory.

The proof state is separate from the inventory entry. A receipt records the build, skin file, palette, size, display grade and captured states. Missing or stale receipts draw as unverified. An inventory entry does not become verified merely because its owner calls a skin helper.

Use source checks as a ratchet for direct material decisions and stale owner/fixture references, with review for newly introduced render paths. Build a native atlas from the real app's existing fixture hooks, reusing the hidden-window helper where supported. Provide a portable isolated-window fallback; a different user must not need Parker's monitor names, private paths, tokens or agent fleet.

## The Authoring Skill

`td-skin` should take a visual reference or description, establish which traits follow the palette, and author one custom skin file. It validates with TD's resolver, launches the native atlas, presents a commentable review and installs only the selected local file. It preserves the user's previous skin and explains how to restore it. It follows existing permission and deployment settings rather than introducing a universal confirmation flow.

Publish one canonical skill through `agent-skills`, with TD exporting the versioned catalogue. Local discovery uses the existing parity links. The skill has no provider-specific dependency on Claude or Codex and can use either TD MCP or its CLI. Its references explain the vocabulary and proof format; they do not duplicate TD's inventory. A supported user look requires no Rust edit. A new rendering strategy is a separate core contribution.

## Least Confident Decisions

1. **How much material data to expose first.** Start with knobs already present in Chalk's renderer, then prove them with a visibly different second look. Avoid a generic shader or plugin language.
2. **How many typography roles are needed.** Reuse `Step` and `Type`. Add family/optical policy only where the first two looks and code/number fields demonstrate a need.
3. **How far the catalogue can enforce coverage.** Owner checks and a material-branch ratchet are useful but incomplete. Native fixtures and changed-renderer review remain necessary.
4. **Which existing glyphs keep their own visual policy.** State colours, palette swatches, terminal grids, hosted documents and media must retain their meaning or content. Their surrounding controls still belong to the skin.

## Slices

1. **Tracer:** route the Subscriptions shell, choice, action, caption and meter through the shared vocabulary. Add its catalogue entry and a native dark/pale fixture. Change one material/type knob in a custom file and show the real UI update without editing that renderer.
2. **Material and type:** move remaining visual decisions behind the shared boundary; parameterise existing paper variations and remove Caveat scaling from the material predicate. Keep shipped appearances and configuration compatible. Verify choice/action distinction and note decoration/colour policy.
3. **Coverage:** migrate the catalogue's remaining owned chrome, including workbench, pane menus, FOCUS frame and document controls. Retain measured terminal/reader geometry, semantic data colour and hosted content. Add fixtures and ratchet checks as each family moves; do not mark an unchecked family complete.
4. **Skill:** package `td-skin` and the reusable native atlas. Prove a second custom look through a file-only authoring request from a clean environment. Inspect that no application Rust changed for that look, then test reload, persistence and restore.

Each slice ends with a running native result and retained evidence. Source tests cover dispatch, inheritance, unknown keys, role semantics and palette/state separation. Native checks cover dark/pale palettes, narrow layouts, UI/text scaling, flat/warped content, active/hover/disabled/unset/unknown states and at least one non-English label set. Measure performance against the existing material on the same fixture; no zero-overhead claim is established by this audit.

## What I Would Do Differently

Start with this catalogue and the actual inherited display grade. Treat material, palette and type as independent from the first prototype. Model a choice and a raised action separately before drawing them. Judge the real font and hit-target geometry in a native strip early. Capture the full native atlas before calling a material complete; whole-suite test counts do not establish visual coverage. Commit the capture/review recipe with portable inputs as soon as it works.

The original chrome-skin plan already required that call sites avoid skin-specific branches. Chalk did not finish that adoption. This proposal completes that boundary and tests the authoring claim with another look before promoting the workflow as a reusable skill.
