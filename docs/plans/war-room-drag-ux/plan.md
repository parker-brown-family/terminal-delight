# War Room Drag UX

The operator needs to see where a member can go while dragging it. Dropping on the pool or bin removes it from its organisation and revokes its access relationships. Clicking away from a context menu must dismiss it immediately.

## Behaviour and Sources

Research read on 2026-10-25. These are design inputs; WCAG describes web accessibility, used here as a benchmark for the native app, not a compliance claim.

| Established behaviour | Application here | Source |
| --- | --- | --- |
| Make draggable objects and destinations recognisable. | Grab cursor on agent cards; low phosphor on valid destinations as soon as dragging starts. | [Atlassian design guidance](https://atlassian.design/components/pragmatic-drag-and-drop/design-guidelines/) |
| Show both acquisition and the prospective result. | Dim the original card; keep a small offset preview; strengthen the hovered target and name its action. | [NN/g drag-and-drop research](https://www.nngroup.com/articles/drag-drop/) |
| Keep previews compact and away from the pointer. | Offset preview 16px right and 8px down; keep the source card's dimensions fixed. | [Atlassian drag previews](https://atlassian.design/components/pragmatic-drag-and-drop/design-guidelines/) |
| The target determines the operation; commit on release. | Chart assigns, pool and bin remove; no mutation during hover. | [Microsoft drag-and-drop](https://learn.microsoft.com/en-us/windows/apps/develop/data/drag-and-drop) |
| Allow cancellation before completing the operation. | Escape cancels the drag first; release outside a valid target changes nothing. | [W3C pointer cancellation](https://www.w3.org/WAI/WCAG22/Understanding/pointer-cancellation.html) |
| Offer an equivalent pointer action without dragging. | Retain Add to org and a selected-member Remove button; expose removal in the card menu. | [W3C dragging movements](https://www.w3.org/WAI/WCAG22/Understanding/dragging-movements) |
| Return from a menu to its invoking context. | Escape closes the card menu before the War Room; an outside press dismisses immediately. | [W3C menu pattern](https://www.w3.org/WAI/ARIA/apg/patterns/menubar/) |
| Give feedback after an action-triggering drop. | Show which pane was removed and that access is off; keep it visible in the pool. | [Apple drag-and-drop guidance](https://developer.apple.com/design/human-interface-guidelines/drag-and-drop) |

## Implementation

Reuse GPUI's existing drag session, native release hit testing, capture-phase outside handlers, theme shadows and the existing GPU focus blur. `war_room_view.rs` owns card menus, drag feedback and one removal handler. `pane.rs` adds outside-press dismissal to its own menu. Workspace holds transient drag/menu/feedback state. User-facing strings remain in all nine language tables.

Removal clears both directions of chart lines, including bypass budgets; removes organisation membership and the seat; sets `AgentAccess::OffLimits`; dismisses related drafts and pending access requests; and saves. Re-adding creates a fresh seat and preserves Off-limits until the operator explicitly changes it. Existing copy results require care: a request made before removal must not resurrect access or disclose a late answer afterward.

## Least Confident Choices

1. Off-limits on removal is deliberately stricter than the old unseated/readable default. It follows Parker's request to clear settings securely and is visible on the pooled card.
2. Keep the bin in a reserved footer so drag start cannot move the drop targets. Use the same removal label and access consequence on both bin and pool.
3. Reuse one sharp rectangle around the work area for GPU blur. Avoid adding renderer machinery for several separate holes; targets stay sharp and the surrounding workspace is frosted.

## Verification

Drive real GPUI events: card drag to each target, cancel with Escape, invalid release, same-org release, outside presses on menus, inside menu selection, shell-card refusal, and short/wide layouts. Assert access denial, removal of all line kinds, saved-state round trip, no late permission restoration, unchanged card bounds, and idle cleanup. Run formatting, Clippy and both terminal-core suites. Render the native UI in an isolated fixture and inspect idle, drag, hover and removed states before installation.
