# War Room Orgs

**Difficulty: 4/10** — one UI surface, with a small saved grouping record and a migration that must preserve existing connections. One combined plan and one approval.
**Turned out to be: 5/10.** Saved membership and legacy migration stayed small; keyboard focus and native drag coverage needed more care than the initial estimate.

- Combined plan: **APPROVED 2026-10-23**. Parker: “Approve layout, but yes. the relationship builder needs ot be a thing.”
- Review page: `reports/2026-10-23-war-room-orgs.html`.
- Implementation is authorized, including drag-and-drop membership and a relationship builder.

## Slices

- [x] Remove the expanded agent context panel; retain compact pane cards and selection.
- [x] Add saved named orgs, create/rename controls, and membership selection.
- [x] Keep card shape fixed during selection and dragging; drag membership with a lightweight card preview.
- [x] Add connector dragging and an explicit source → target relationship editor with See / Read / Ask and removal.
- [x] Preserve legacy seats and lines, verify global search and session restore, then build and install.

## Resume

Parker rejected the large selected-agent response card in his screenshot and asked to make and label orgs. The proposed model gives each pane one org. Existing seats and lines open together as an unnamed org whose label is unknown until a person names it. Explicit connections continue to control permissions. Search remains global and identifies the org containing each match. Read the plan and the page's note/concur map before implementation.

Parker's amendment: “I am a fan of cards NOT transforming and drag and drop (even if it means making a card being dragged into a LOW RES render of the card to save on graphics and render power.” Keep cards fixed in shape; use a lightweight drag representation. The relationship builder is part of this approved build, not a later follow-up.

The search/card feature has merged into `dev/0.5.0`. This org feature continues on `feat/war-room-orgs`. Implementation, native drag tests, the full serial suite (2,801 passed; 17 ignored), Clippy for both cores and the optimized build pass. The optimized build is installed as `td-fdc799ee-0.5-war-room-orgs` and passed a hidden native window smoke check. The running user window still needs reopening to load it. The walkthrough and capture are in [the build report](../../../reports/2026-10-23-war-room-orgs-built.md).

## Layout amendment

Parker’s boundary-clipping screenshot on 2026-10-23 replaces the stacked chart and available-pane list with two side-by-side tiles: org chart on the left, pane pool on the right. The frame and visible headers stay fixed; the contents of each tile scroll independently. This is an authorized layout correction within the approved feature. Initial difficulty: 3/10.
