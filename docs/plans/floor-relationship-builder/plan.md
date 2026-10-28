# Relationship Builder

**Difficulty: 6/10.** One combined plan. Reader: Parker, reviewing the visible behavior before implementation.

**Approved with amendments 2026-10-28.** The following remarks take precedence over the original prototype and the superseded picker paragraphs below. Parker said to keep going after making them.

## Amendments

Keep a fixed candidate relationship surface visible on the Floor, including its empty actor and target slots. Activation, selection and added targets must not move or resize the chart, pool or filter. The actual chart supplies the selectable target cards; remove the separate candidate list. Keep the filter text and keyboard editing working throughout composition. A dragged pool card may fill actor or target slots or join a pending target selection on the chart. Its membership remains unchanged until Submit. Validate the whole draft first, then commit staged memberships and per-target lines together. Cancellation makes no membership change. Group window access switches, navigation, finding panes and relationship actions by purpose, with breathing space between them. The user concurred with one verb for the set.

The existing single-org line store remains the persistence boundary. Transient draft members may refer to an existing seat or a live pool pane. Pool candidates from another org keep the existing cross-org routes rather than silently moving. Live-pool identity, current membership and agent eligibility must be checked again on Submit. Missing targets or changed ownership leave the draft visible and apply nothing.

## Purpose

The current Connect form blends into the page, uses From/To labels and text rows, and only accepts one target. Dragging the card body carries a membership payload that another card does not accept. Parker wants a distinct on-page builder using the active element glass: ACTOR → CAN {VERB} → TARGETS. A selected card fills the actor. Empty slots and a plus button make adding several targets explicit.

## Shape

Keep the builder inside the Threshing Floor, above the chart. Give it the existing skin's active action backing, lit edge, spacing and one clear title. The design prototype approximates the supplied glass reference; production reuses `Skin::action` and `Skin::choice` rather than adding a material. Actor, verb and targets read horizontally when space permits and stack in that order when narrow. Limit the card-picker height so the chart remains reachable.

Connect with no selection opens two empty card slots and the actor picker. With a selection, it prefills that actor and opens the target picker. Card-body drop on another member and Alt+left drag from anywhere on a seated card prefill both endpoints. Alt-drag paints a line from the actor to the pointer and highlights the valid target, keeping the card fixed. Escape and invalid/self drops clear the gesture and change nothing. Ordinary drag onto the pool, bin or org background retains the existing membership paths. A direct member hit takes precedence over its surrounding org drop zone. Cross-org member drops retain their existing scope rules; the local builder validates current-org members, while cross-org routes continue through the System Map.

An empty actor remains absent. Verb starts absent for a new relationship; Save requires an explicit see/read/ask/prompt choice. Editing a line loads its saved verb. All options are identifiable cards with place, title, program and state; unavailable measurements remain labelled unknown. The target picker excludes the actor and already chosen targets. Plus opens that same picker and stays available after each choice. Removing a target returns it to the picker. Changing actor removes that actor from the target set and retains the other targets, with visible feedback. The global pane filter is preserved; the picker must still expose eligible members even when the board filter hides them.

One Save applies the selected verb to each listed target. Show the relationship count and any existing lines that will change before Save. Validate all members, stable seat identities, scope, duplicates and self-links before mutating the chart; if a target disappeared, retain the draft and name it. Use the existing pair-based line store, so no saved group or schema migration is required. Invalidate existing per-pair bypass/copy receipts for every changed pair and save once. The existing pane access limits remain authoritative and appear on option cards; choosing prompt never silently opens an off-limits or read-only pane. A line-label edit uses the same visual builder with fixed endpoints and one target, preserving removal; plus belongs to creation mode.

## Interfaces

Keep the existing pair draft if another caller needs it; introduce a distinct batch draft for this surface. Proposed signatures, without implementation bodies:

```rust
struct RelationshipBatch {
    actor: Option<SeatId>,
    targets: Vec<SeatId>,
    verb: Option<Rung>,
}

fn validate_batch(draft: &RelationshipBatch, orgs: &Orgs)
    -> Result<Vec<(SeatId, SeatId, Rung)>, RelationshipError>;
```

`threshing_floor.rs` owns pure batch validation. `threshing_floor_view.rs` owns picker state, card rendering, gesture routing, live-leaf revalidation and applying the validated pairs. `main.rs` holds transient workspace state. `lang/chrome.rs` supplies the labels in all nine locales. No new dependency or model call is needed. `Skin` supplies material unchanged.

## Least Confident Decisions

1. A single verb applies to all targets. Existing lines are replaced only after the summary identifies them; mixed verbs require another action. This keeps the sentence unambiguous.
2. The picker shows the current org's full eligible membership independently of the board's filter. Otherwise a selected actor can appear to have no targets merely because the person filtered to that actor.
3. Changing actor keeps every target except the new actor. Clearing them all would discard work; keeping a self-target would be invalid.

## Slices and Proof

1. One native builder opens from Connect with or without selection, card drop, connector drag and Alt-drag. Native event tests must detect direct-target precedence, a visible tether, fixed card bounds, self/outside-drop cancellation and Escape cleanup. The first slice ends with a working single-pair save.
2. Add and remove target cards; reject duplicates and self-links; keep absent verb distinct from a chosen verb. Save three targets, inspect the three directed lines, reload them, and verify that the reverse lines are absent. If one target vanishes before Save, assert the chart is unchanged. Cover existing-line replacement, receipt revocation for each changed pair and unchanged unrelated lines.
3. Reuse active skin materials; translate labels; check card slots, picker bounds and the save row at narrow and wide widths. Verify membership drops and cross-org routes keep working. Run formatting, both core test suites and Clippy, then prepare native screenshots and the build receipt. No live window restart is part of this planning turn.

## Evidence

Observed in the current integration source: `PaneDrag` is registered on the card body while the card accepts only `RelationDrag`; `RelationDraft` holds one optional target; `threshing_floor_choose_endpoint` replaces that target. `threshing_floor_begin_relation` already accepts a selected source, and the target list already excludes it after that source exists. The fix must preserve these working checks while making the behavior visible and handling multiple targets.

The interactive review page has fictional extra members to demonstrate multiple targets; their state readings are explicitly sample data. It does not save permissions or run native gestures. Native interaction, persistence and permission checks remain work for implementation.
