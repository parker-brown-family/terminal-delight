# War Room Orgs

**Difficulty: 4/10** — one surface with persisted groups. One approval before implementation.

**APPROVED 2026-10-23**, with fixed-shape cards, drag-and-drop org membership, and a relationship builder. The builder uses a connector drag from source to target, followed by an explicit See / Read / Ask selection. Clicking an existing edge label edits or removes it. A Connect action with source and target selection provides the same operation without dragging.

Parker's request: “This giant card for … the agent context must die… We need to be able to MAKE AND LABEL orgs.”

The selected-agent context panel is removed completely. Selecting a pane highlights its compact card and exposes a small action row. The cards keep project, tab, pane title, state, age and current context tokens. Their shape stays fixed during selection and dragging. No response text expands beside the chart. A lightweight text-and-outline drag preview avoids repeatedly rendering the full card.

An org is a saved, named chart containing selected panes. A selector switches orgs; New org creates an empty org from a typed label, and Rename edits the current label. Add to org assigns an available pane to the chosen org. A pane belongs to one org in this proposal. Removing it uses the existing unseat action, which removes its connections; it becomes available to add elsewhere.

Existing layouts preserve their exact seats and connections together in one unnamed org. Its stored name is absent, rendered as “Unnamed org,” until a person names it. Do not infer membership from projects, tabs or titles. Missing or unreadable membership remains visible as unassigned, with any existing connection identified rather than silently discarded.

Org membership creates no permission. The existing explicit lines remain the permission source. Creating or renaming an org does not grant access. The editor connects members of the selected org; switching orgs clears a half-drawn line. Removing or moving a pane must disclose that its existing connections are removed, as the current unseat action does.

Search stays global across project, tab name and pane title. A result in another org names that org and offers Open org. Opening that result selects its org and clears the search so the full chart is visible.

## The Shape

- `app/src/war_room_view.rs`: remove `war_room_card` and its summary gathering; render org selection, create/rename input, member cards and available-pane assignment. Reuse `EditBuffer` and the existing fuzzy matcher.
- `app/src/warroom.rs`: org identity and membership rules alongside stable seat identities. Keep `Chart` as the explicit connection store.
- `app/src/main.rs`: persist org records alongside saved lines and seats; hold the selected org/editor state in `Workspace`; migrate old layouts without changing connections.
- `app/src/lang/chrome.rs` and language checks: all new labels in nine languages.

Proposed saved record, before implementation:

```rust
struct Org {
    id: OrgId,
    name: Option<String>,
    seats: Vec<SeatId>,
}
```

The absent label represents an existing chart nobody has named. New org creation requires a nonblank name. IDs survive renaming and pane respawn. Missing org metadata distinguishes an old layout needing migration from an explicitly empty org list.

## Least Confident Decisions

1. One org per pane. This keeps a pane's membership and its connections visible together. Shared membership across multiple orgs would need separate rules for which connections each chart owns.
2. Existing seats migrate together into one unnamed org. This preserves the chart without inventing the user's intended grouping.

## Slices and Verification

1. Remove the expanded context panel and verify that selecting a pane never draws response text while connection drawing still works.
2. Create, rename, switch and assign panes to orgs, saved and restored by stable identity. Show the native UI in a hidden test window.
3. Verify legacy migration preserves every seat and line, naming creates no grants, switching cancels pending connections, global search finds other-org panes, unknown readings remain unknown, and restore keeps membership. Run formatting, Clippy and the appropriate full suite before installing the optimized development build.

Approval is one decision on the complete proposal. The interactive review sketch is in `reports/2026-10-23-war-room-orgs.html`.
